//! Runs a workflow. Jobs start once the jobs they need are done; a matrix
//! job runs once per combination. Each job gets a container of its own
//! (the runner image, or its `container:`), with its services sharing the
//! container's network, so they answer on `localhost` as on GitHub. Steps
//! run one by one; between them, `GITHUB_ENV`, `GITHUB_OUTPUT` and
//! `GITHUB_PATH` are read back.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::models::{
    ContainerCreateBody, HealthConfig, HostConfig, Mount, MountType, SystemInfo, SystemVersion,
};
use bollard::query_parameters::{
    CreateContainerOptionsBuilder, ListContainersOptions, RemoveContainerOptions,
    StartContainerOptions, UploadToContainerOptions,
};
use futures_util::StreamExt;
use serde_json::{Map, Value, json};

use super::actions::{self, Builtin};
use super::commands::{self, Output};
use super::context::{self, GitInfo};
use super::expr::{self, Context, Status};
use super::workflow::{Container, Job, Step, Workflow};
use super::{CI_LABEL, CiEvent, LineKind, LogLine, State, image, matrix, workspace};
use crate::{DockerClient, describe};

/// A run to make.
#[derive(Clone, PartialEq)]
pub struct RunRequest {
    pub run: u64,
    pub project: PathBuf,
    pub workflow: Workflow,
    /// `push`, `pull_request` or `workflow_dispatch`.
    pub event: String,
    pub inputs: Vec<(String, String)>,
    pub secrets: Vec<(String, String)>,
}

/// Never prints the secrets' values.
impl std::fmt::Debug for RunRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let secrets: Vec<&str> = self.secrets.iter().map(|(k, _)| k.as_str()).collect();
        f.debug_struct("RunRequest")
            .field("run", &self.run)
            .field("project", &self.project)
            .field("workflow", &self.workflow.file)
            .field("event", &self.event)
            .field("inputs", &self.inputs)
            .field("secrets", &secrets)
            .finish()
    }
}

type Emit<'a> = &'a (dyn Fn(CiEvent) + Send + Sync);

/// `github.token` when the run has no GITHUB_TOKEN secret.
pub const NO_TOKEN: &str = "ferrodock-local-ci-no-token";

/// Runs asked to stop.
static CANCELLED: Mutex<Vec<u64>> = Mutex::new(Vec::new());

fn cancelled(run: u64) -> bool {
    CANCELLED.lock().is_ok_and(|c| c.contains(&run))
}

/// Stops a run: its steps end, and its containers are removed.
pub async fn cancel_run(client: &DockerClient, run: u64) {
    if let Ok(mut c) = CANCELLED.lock() {
        c.push(run);
    }
    let _ = client.remove_labelled(&run.to_string()).await;
}

/// Where the runner image keeps things; a `container:` job uses GitHub's
/// paths for containers instead.
pub(super) struct Paths {
    pub(super) workspace: String,
    home: String,
    pub(super) temp: String,
    /// `runner` in the runner image; the image's own user otherwise.
    user: String,
}

impl Paths {
    fn new(repository: &str, in_container: bool) -> Self {
        let name = repository.rsplit('/').next().unwrap_or("project");
        if in_container {
            Self {
                workspace: format!("/__w/{name}/{name}"),
                home: "/github/home".into(),
                temp: "/__w/_temp".into(),
                user: String::new(),
            }
        } else {
            Self {
                workspace: format!("/home/runner/work/{name}/{name}"),
                home: "/home/runner".into(),
                temp: "/home/runner/work/_temp".into(),
                user: "runner".into(),
            }
        }
    }

    pub(super) fn file(&self, name: &str) -> String {
        format!("{}/_runner_file_commands/{name}", self.temp)
    }
}

pub(super) struct Run<'a> {
    client: &'a DockerClient,
    pub(super) req: &'a RunRequest,
    emit: Emit<'a>,
    git: GitInfo,
    next_job: AtomicUsize,
}

/// A script that runs when the job ends, e.g. saving `actions/cache`.
pub(super) struct Post {
    pub(super) name: String,
    pub(super) run: PostRun,
    pub(super) env: Vec<String>,
    /// Its `if:`; none is `success()`.
    pub(super) condition: Option<String>,
}

pub(super) enum PostRun {
    /// A bash script.
    Script(String),
    /// A program and its arguments, run as is (no shell in between, so
    /// variables like `INPUT_WHO-TO-GREET` reach it).
    Command(Vec<String>),
}

/// What a finished job leaves for the jobs that need it.
#[derive(Clone)]
struct Outcome {
    state: State,
    outputs: Map<String, Value>,
}

/// Runs the workflow, telling `emit` how it goes. Returns how it ended.
pub async fn run_workflow(client: &DockerClient, req: RunRequest, emit: Emit<'_>) -> State {
    let run = Run {
        client,
        req: &req,
        emit,
        git: context::git_info(&req.project),
        next_job: AtomicUsize::new(0),
    };
    let mut results: HashMap<String, Outcome> = HashMap::new();
    let mut pending: Vec<&Job> = req.workflow.jobs.iter().collect();
    while !pending.is_empty() {
        let ready: Vec<&Job> = pending
            .iter()
            .copied()
            .filter(|j| j.needs.iter().all(|n| results.contains_key(n)))
            .collect();
        if ready.is_empty() {
            // Needs that never finish: a job that isn't there, or a loop.
            for job in pending {
                let idx = run.announce(&job.id);
                run.setup_message(
                    idx,
                    &format!("It needs {}, which can't run.", job.needs.join(", ")),
                    LineKind::Error,
                );
                run.finish_job(idx, State::Skipped);
                results.insert(
                    job.id.clone(),
                    Outcome {
                        state: State::Skipped,
                        outputs: Map::new(),
                    },
                );
            }
            break;
        }
        pending.retain(|j| !ready.iter().any(|r| r.id == j.id));
        let outcomes =
            futures_util::future::join_all(ready.iter().map(|job| run.job(job, &results))).await;
        for (job, outcome) in ready.iter().zip(outcomes) {
            results.insert(job.id.clone(), outcome);
        }
    }
    let states: Vec<State> = results.values().map(|o| o.state).collect();
    let state = if cancelled(req.run) || states.contains(&State::Cancelled) {
        State::Cancelled
    } else if states.contains(&State::Failure) {
        State::Failure
    } else {
        State::Success
    };
    let _ = client.remove_labelled(&req.run.to_string()).await;
    if let Ok(mut c) = CANCELLED.lock() {
        c.retain(|r| *r != req.run);
    }
    emit(CiEvent::Done {
        run: req.run,
        state,
    });
    state
}

/// `timeout-minutes` as a duration (kept within a sane range).
fn minutes_to_duration(minutes: f64) -> Duration {
    let minutes = if minutes.is_finite() {
        minutes.clamp(0.0, 100_000.0)
    } else {
        360.0
    };
    Duration::from_secs_f64(minutes * 60.0)
}

/// `needs.<job>.result` and `.outputs` for the jobs a job needs.
fn needs_context(job: &Job, results: &HashMap<String, Outcome>) -> Value {
    Value::Object(
        job.needs
            .iter()
            .filter_map(|n| results.get(n).map(|o| (n, o)))
            .map(|(n, o)| {
                let result = match o.state {
                    State::Success => "success",
                    State::Failure => "failure",
                    State::Cancelled => "cancelled",
                    _ => "skipped",
                };
                (n.clone(), json!({"result": result, "outputs": o.outputs}))
            })
            .collect(),
    )
}

fn result_text(state: State) -> &'static str {
    match state {
        State::Success => "success",
        State::Failure => "failure",
        State::Cancelled => "cancelled",
        _ => "skipped",
    }
}

impl Run<'_> {
    fn emit(&self, event: CiEvent) {
        (self.emit)(event);
    }

    /// A new job, with its "Set up job" step.
    fn announce(&self, name: &str) -> usize {
        let job = self.next_job.fetch_add(1, Ordering::Relaxed);
        let run = self.req.run;
        self.emit(CiEvent::Job {
            run,
            job,
            name: name.to_owned(),
        });
        self.emit(CiEvent::Step {
            run,
            job,
            step: 0,
            name: "Set up job".into(),
        });
        job
    }

    fn setup_message(&self, job: usize, text: &str, kind: LineKind) {
        self.emit(CiEvent::Log {
            run: self.req.run,
            job,
            step: 0,
            lines: vec![LogLine {
                text: text.to_owned(),
                kind,
            }],
        });
    }

    fn finish_job(&self, job: usize, state: State) {
        let run = self.req.run;
        let step_state = if state == State::Skipped {
            State::Skipped
        } else {
            State::Failure
        };
        self.emit(CiEvent::StepState {
            run,
            job,
            step: 0,
            state: step_state,
        });
        self.emit(CiEvent::JobState { run, job, state });
    }

    /// The contexts every expression of a job sees.
    fn base_context(&self, job_id: &str, needs: Value) -> Map<String, Value> {
        let inputs: Map<String, Value> = self
            .req
            .workflow
            .inputs
            .iter()
            .map(|i| {
                let value = self
                    .req
                    .inputs
                    .iter()
                    .find(|(k, _)| *k == i.name)
                    .map_or_else(|| i.default.clone(), |(_, v)| v.clone());
                let value = match i.kind.as_str() {
                    "boolean" => Value::Bool(value == "true"),
                    _ => Value::String(value),
                };
                (i.name.clone(), value)
            })
            .collect();
        let mut github = context::github(
            &self.git,
            &self.req.workflow.name,
            &self.req.event,
            &self.req.inputs,
            self.req.run,
            &std::env::var("USER")
                .or_else(|_| std::env::var("USERNAME"))
                .unwrap_or_else(|_| "you".into()),
            "",
        );
        github["job"] = Value::String(job_id.to_owned());
        // On GitHub there's always a token, and many actions want one. Here
        // it's the GITHUB_TOKEN secret, or a stand-in: actions that only
        // need one to be there run, and calls to GitHub's API are refused.
        let token = self
            .req
            .secrets
            .iter()
            .find(|(k, _)| k == "GITHUB_TOKEN")
            .map_or(NO_TOKEN, |(_, t)| t.as_str());
        github["token"] = Value::String(token.to_owned());
        let mut data = Map::new();
        data.insert("github".into(), github);
        data.insert("needs".into(), needs);
        data.insert("inputs".into(), Value::Object(inputs));
        data.insert(
            "secrets".into(),
            Value::Object(
                self.req
                    .secrets
                    .iter()
                    .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                    .collect(),
            ),
        );
        data.insert("vars".into(), Value::Object(Map::new()));
        data.insert("env".into(), Value::Object(Map::new()));
        data
    }

    async fn job(&self, job: &Job, results: &HashMap<String, Outcome>) -> Outcome {
        let needs = needs_context(job, results);
        let need_states: Vec<State> = job
            .needs
            .iter()
            .filter_map(|n| results.get(n))
            .map(|o| o.state)
            .collect();
        let status = if need_states.contains(&State::Cancelled) || cancelled(self.req.run) {
            Status::Cancelled
        } else if need_states.iter().any(|s| *s != State::Success) {
            Status::Failure
        } else {
            Status::Success
        };
        let data = self.base_context(&job.id, needs.clone());
        let hash = |p: &[String]| workspace::hash_files(&self.req.project, p);
        let ctx = Context {
            data: &data,
            status,
            hash_files: Some(&hash),
        };
        let skip = |why: &str| {
            let idx = self.announce(job.name.as_deref().unwrap_or(&job.id));
            self.setup_message(idx, why, LineKind::Info);
            self.finish_job(idx, State::Skipped);
            Outcome {
                state: State::Skipped,
                outputs: Map::new(),
            }
        };
        match expr::condition(job.condition.as_deref(), &ctx) {
            Ok(true) => {}
            Ok(false) => {
                return skip(
                    "Skipped: its if: condition isn't met, or a job it needs didn't succeed.",
                );
            }
            Err(e) => return skip(&format!("Skipped: its if: can't be read: {e}")),
        }
        if let Some(uses) = &job.uses {
            return skip(&format!(
                "Skipped: reusable workflows ({uses}) don't run in Local CI yet."
            ));
        }
        // The matrix, evaluated if it's an expression.
        let matrix = match &job.matrix {
            Some(Value::String(text)) => match expr::interpolate(text, &ctx)
                .and_then(|t| serde_json::from_str::<Value>(&t).map_err(|e| e.to_string()))
            {
                Ok(m) => Some(m),
                Err(e) => return skip(&format!("Its matrix can't be read: {e}")),
            },
            other => other.clone(),
        };
        let combos = match matrix::expand(matrix.as_ref()) {
            Ok(c) => c,
            Err(e) => return skip(&format!("Its matrix can't be used: {e}")),
        };
        let total = combos.len();
        let failed_fast = AtomicBool::new(false);
        let limit = job.max_parallel.unwrap_or(4).max(1);
        let runs = futures_util::stream::iter(combos.into_iter().enumerate().map(|(n, combo)| {
            let (data, failed_fast) = (&data, &failed_fast);
            async move {
                if job.fail_fast && failed_fast.load(Ordering::Relaxed) {
                    let idx = self.announce(&matrix::job_name(&job.id, &combo));
                    self.setup_message(
                        idx,
                        "Cancelled: another job of this matrix failed (fail-fast).",
                        LineKind::Info,
                    );
                    self.finish_job(idx, State::Cancelled);
                    return Outcome {
                        state: State::Cancelled,
                        outputs: Map::new(),
                    };
                }
                let outcome = self.combination(job, combo, data.clone(), n, total).await;
                if outcome.state == State::Failure {
                    failed_fast.store(true, Ordering::Relaxed);
                }
                outcome
            }
        }))
        .buffer_unordered(limit)
        .collect::<Vec<Outcome>>()
        .await;
        let states: Vec<State> = runs.iter().map(|o| o.state).collect();
        let state = if states.contains(&State::Failure) && !job.continue_on_error {
            State::Failure
        } else if states.contains(&State::Cancelled) {
            State::Cancelled
        } else if states.iter().all(|s| *s == State::Skipped) {
            State::Skipped
        } else {
            State::Success
        };
        let mut outputs = Map::new();
        for o in runs {
            outputs.extend(o.outputs);
        }
        Outcome { state, outputs }
    }

    /// One job of a matrix (or the job, without one).
    async fn combination(
        &self,
        job: &Job,
        combo: matrix::Combination,
        mut data: Map<String, Value>,
        index: usize,
        total: usize,
    ) -> Outcome {
        let run = self.req.run;
        data.insert("matrix".into(), Value::Object(combo.clone()));
        data.insert(
            "strategy".into(),
            json!({"fail-fast": job.fail_fast, "job-index": index, "job-total": total, "max-parallel": job.max_parallel.unwrap_or(total)}),
        );
        let hash = |p: &[String]| workspace::hash_files(&self.req.project, p);
        let text = |t: &str, data: &Map<String, Value>| {
            expr::interpolate(
                t,
                &Context {
                    data,
                    status: Status::Success,
                    hash_files: Some(&hash),
                },
            )
            .unwrap_or_else(|_| t.to_owned())
        };
        let name = job
            .name
            .as_deref()
            .map_or_else(|| matrix::job_name(&job.id, &combo), |n| text(n, &data));
        let idx = self.announce(&name);
        self.emit(CiEvent::JobState {
            run,
            job: idx,
            state: State::Running,
        });
        let step_names: Vec<String> = job
            .steps
            .iter()
            .map(|s| match (&s.name, &s.run, &s.uses) {
                (Some(n), _, _) => text(n, &data),
                (None, Some(r), _) => format!("Run {}", r.lines().next().unwrap_or("").trim()),
                (None, None, Some(u)) => format!("Run {u}"),
                _ => "Step".into(),
            })
            .collect();
        for (n, name) in step_names.iter().enumerate() {
            self.emit(CiEvent::Step {
                run,
                job: idx,
                step: n + 1,
                name: name.clone(),
            });
        }
        let mut job_run = JobRun::new(self, job, idx, data);
        let state = job_run.go(&step_names).await;
        let outputs = job_run.outputs();
        job_run.clean_up().await;
        self.emit(CiEvent::JobState {
            run,
            job: idx,
            state,
        });
        Outcome { state, outputs }
    }
}

/// A job (one matrix combination) while it runs.
pub(super) struct JobRun<'r, 'a> {
    pub(super) run: &'r Run<'a>,
    job: &'r Job,
    idx: usize,
    data: Map<String, Value>,
    pub(super) paths: Paths,
    container: String,
    services: Vec<String>,
    /// The `PATH` of the image, before `GITHUB_PATH` adds to it.
    base_path: String,
    added_paths: Vec<String>,
    /// From `GITHUB_ENV`.
    exported: Vec<(String, String)>,
    steps: Map<String, Value>,
    pub(super) status: Status,
    /// What runs when the job ends: actions' `post` scripts.
    pub(super) posts: Vec<Post>,
    /// How deep in composite actions the running step is.
    pub(super) nesting: usize,
    next_step: usize,
    /// When the job runs out of time (`timeout-minutes`, 360 by default).
    deadline: Option<Instant>,
}

impl<'r, 'a> JobRun<'r, 'a> {
    fn new(run: &'r Run<'a>, job: &'r Job, idx: usize, data: Map<String, Value>) -> Self {
        let paths = Paths::new(&run.git.repository, job.container.is_some());
        Self {
            run,
            job,
            idx,
            data,
            paths,
            container: String::new(),
            services: Vec::new(),
            base_path: "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".into(),
            added_paths: Vec::new(),
            exported: Vec::new(),
            steps: Map::new(),
            status: Status::Success,
            posts: Vec::new(),
            nesting: 0,
            next_step: job.steps.len() + 1,
            deadline: None,
        }
    }

    fn id(&self) -> u64 {
        self.run.req.run
    }

    pub(super) fn log(&self, step: usize, lines: Vec<LogLine>) {
        if !lines.is_empty() {
            self.run.emit(CiEvent::Log {
                run: self.id(),
                job: self.idx,
                step,
                lines,
            });
        }
    }

    pub(super) fn info(&self, step: usize, text: impl Into<String>) {
        self.log(step, vec![LogLine::info(text)]);
    }

    fn step_state(&self, step: usize, state: State) {
        self.run.emit(CiEvent::StepState {
            run: self.id(),
            job: self.idx,
            step,
            state,
        });
    }

    fn ctx_data(&self, step_env: &[(String, String)]) -> Map<String, Value> {
        let mut data = self.data.clone();
        let mut env: Map<String, Value> = Map::new();
        let hash = |p: &[String]| workspace::hash_files(&self.run.req.project, p);
        // Each level of env sees the ones above it.
        for (level, vars) in [
            ("workflow", &self.run.req.workflow.env),
            ("job", &self.job.env),
        ] {
            let _ = level;
            for (k, v) in vars {
                data.insert("env".into(), Value::Object(env.clone()));
                let value = expr::interpolate(
                    v,
                    &Context {
                        data: &data,
                        status: self.status,
                        hash_files: Some(&hash),
                    },
                )
                .unwrap_or_else(|_| v.clone());
                env.insert(k.clone(), Value::String(value));
            }
        }
        for (k, v) in &self.exported {
            env.insert(k.clone(), Value::String(v.clone()));
        }
        for (k, v) in step_env {
            data.insert("env".into(), Value::Object(env.clone()));
            let value = expr::interpolate(
                v,
                &Context {
                    data: &data,
                    status: self.status,
                    hash_files: Some(&hash),
                },
            )
            .unwrap_or_else(|_| v.clone());
            env.insert(k.clone(), Value::String(value));
        }
        data.insert("env".into(), Value::Object(env));
        data.insert("steps".into(), Value::Object(self.steps.clone()));
        data.insert(
            "job".into(),
            json!({"status": result_text(match self.status { Status::Success => State::Success, Status::Failure => State::Failure, Status::Cancelled => State::Cancelled })}),
        );
        data.insert(
            "runner".into(),
            context::runner(&self.paths.temp, actions::TOOL_CACHE),
        );
        if let Some(gh) = data.get_mut("github") {
            gh["workspace"] = Value::String(self.paths.workspace.clone());
        }
        data
    }

    /// The environment of a step's process: GitHub's own variables, then
    /// the workflow's.
    pub(super) fn process_env(
        &self,
        data: &Map<String, Value>,
        step_id: &str,
        extra: &[(String, String)],
    ) -> Vec<String> {
        let gh = &data["github"];
        let get = |k: &str| expr::to_text(&gh[k]);
        let mut env = vec![
            "CI=true".to_owned(),
            "GITHUB_ACTIONS=true".to_owned(),
            format!("HOME={}", self.paths.home),
            format!("GITHUB_WORKSPACE={}", self.paths.workspace),
            format!(
                "GITHUB_EVENT_PATH={}/_github_workflow/event.json",
                self.paths.temp
            ),
            format!("GITHUB_ENV={}", self.paths.file("env")),
            format!("GITHUB_OUTPUT={}", self.paths.file("output")),
            format!("GITHUB_PATH={}", self.paths.file("path")),
            format!("GITHUB_STEP_SUMMARY={}", self.paths.file("summary")),
            format!("GITHUB_STATE={}", self.paths.file("state")),
            format!("GITHUB_ACTION={step_id}"),
            format!("GITHUB_JOB={}", self.job.id),
            format!("RUNNER_TEMP={}", self.paths.temp),
            format!("RUNNER_TOOL_CACHE={}", actions::TOOL_CACHE),
            "RUNNER_OS=Linux".to_owned(),
            format!(
                "RUNNER_ARCH={}",
                expr::to_text(&context::runner("", "")["arch"])
            ),
            "RUNNER_NAME=Ferrodock Local CI".to_owned(),
            "ImageOS=ubuntu24".to_owned(),
        ];
        for key in [
            "event_name",
            "ref",
            "ref_name",
            "ref_type",
            "head_ref",
            "base_ref",
            "sha",
            "repository",
            "repository_owner",
            "actor",
            "triggering_actor",
            "workflow",
            "run_id",
            "run_number",
            "run_attempt",
            "server_url",
            "api_url",
            "graphql_url",
        ] {
            env.push(format!("GITHUB_{}={}", key.to_uppercase(), get(key)));
        }
        if let Some(Value::Object(vars)) = data.get("env") {
            for (k, v) in vars {
                env.push(format!("{k}={}", expr::to_text(v)));
            }
        }
        for (k, v) in extra {
            env.push(format!("{k}={v}"));
        }
        let mut path: Vec<String> = self.added_paths.iter().rev().cloned().collect();
        path.push(self.base_path.clone());
        env.push(format!("PATH={}", path.join(":")));
        env
    }

    fn masks(&self) -> Vec<String> {
        self.run
            .req
            .secrets
            .iter()
            .map(|(_, v)| v.clone())
            .collect()
    }

    /// `runs-on`, with expressions like `${{ matrix.os }}` filled in.
    fn runs_on(&self) -> Vec<String> {
        let ctx = Context {
            data: &self.data,
            status: Status::Success,
            hash_files: None,
        };
        self.job
            .runs_on
            .iter()
            .map(|l| expr::interpolate(l, &ctx).unwrap_or_else(|_| l.clone()))
            .collect()
    }

    /// Runs the job. Returns how it ended.
    async fn go(&mut self, step_names: &[String]) -> State {
        let run = self.id();
        self.step_state(0, State::Running);
        if let Some(ubuntu) = self
            .job
            .container
            .is_none()
            .then(|| image::ubuntu_for(&self.runs_on()))
            && let Err(why) = ubuntu
        {
            self.log(
                0,
                vec![LogLine {
                    text: format!("Skipped: {why}."),
                    kind: LineKind::Warning,
                }],
            );
            self.step_state(0, State::Skipped);
            for n in 1..=self.job.steps.len() {
                self.step_state(n, State::Skipped);
            }
            return State::Skipped;
        }
        if let Err(e) = self.set_up().await {
            self.log(
                0,
                vec![LogLine {
                    text: e,
                    kind: LineKind::Error,
                }],
            );
            self.step_state(0, State::Failure);
            for n in 1..=self.job.steps.len() {
                self.step_state(n, State::Skipped);
            }
            return if cancelled(run) {
                State::Cancelled
            } else {
                State::Failure
            };
        }
        self.step_state(0, State::Success);
        let minutes = self.job.timeout_minutes.unwrap_or(360.0);
        self.deadline = Some(Instant::now() + minutes_to_duration(minutes));
        for (n, step) in self.job.steps.iter().enumerate() {
            let number = n + 1;
            if cancelled(run) {
                self.status = Status::Cancelled;
            }
            let state = self.step(number, step, &step_names[n]).await;
            self.step_state(number, state);
        }
        self.post_steps().await;
        match self.status {
            Status::Success => State::Success,
            Status::Failure => State::Failure,
            Status::Cancelled => State::Cancelled,
        }
    }

    async fn set_up(&mut self) -> Result<(), String> {
        let client = self.run.client;
        let hash = |p: &[String]| workspace::hash_files(&self.run.req.project, p);
        let data = self.ctx_data(&[]);
        let text = |t: &str| {
            expr::interpolate(
                t,
                &Context {
                    data: &data,
                    status: Status::Success,
                    hash_files: Some(&hash),
                },
            )
        };
        let image = match &self.job.container {
            Some(c) => {
                let image = text(&c.image)?;
                self.info(0, format!("Container: {image}"));
                self.pull(&image).await?;
                image
            }
            None => {
                let ubuntu = image::ubuntu_for(&self.runs_on())
                    .map_err(|e| format!("This job can't run here: {e}."))?;
                self.info(
                    0,
                    format!(
                        "Runner: Ubuntu {ubuntu} (runs-on: {})",
                        self.runs_on().join(", ")
                    ),
                );
                let mut lines = Vec::new();
                let tag = client
                    .ensure_runner_image(ubuntu, |l| {
                        lines.push(LogLine::info(l.trim_end().to_owned()))
                    })
                    .await;
                self.log(0, lines);
                tag?
            }
        };
        let id = self.id();
        let name = format!("ferrodock-ci-{id}-{}", self.idx);
        let labels = HashMap::from([(CI_LABEL.to_owned(), id.to_string())]);
        let service_hosts: Vec<String> = self
            .job
            .services
            .iter()
            .map(|(sid, _)| format!("{sid}:127.0.0.1"))
            .collect();
        let mut env: Vec<String> = Vec::new();
        if let Some(c) = &self.job.container {
            for (k, v) in &c.env {
                env.push(format!("{k}={}", text(v)?));
            }
        }
        // The engine itself, as on GitHub's runners, so `docker build`,
        // `docker run` and `docker compose` work in a job.
        let engine = (client.docker.info().await, client.docker.version().await);
        let socket = match engine {
            (Ok(info), Ok(version)) if engine_socket(&info, &version) => Some(Mount {
                target: Some(DOCKER_SOCKET.into()),
                source: Some(DOCKER_SOCKET.into()),
                typ: Some(MountType::BIND),
                ..Mount::default()
            }),
            _ => None,
        };
        let mut body = ContainerCreateBody {
            image: Some(image.clone()),
            entrypoint: Some(vec!["tail".into(), "-f".into(), "/dev/null".into()]),
            cmd: Some(Vec::new()),
            env: Some(env),
            working_dir: Some("/".into()),
            user: (!self.paths.user.is_empty()).then(|| self.paths.user.clone()),
            labels: Some(labels.clone()),
            host_config: Some(HostConfig {
                binds: Some(vec![
                    format!("{}:{}", actions::TOOL_CACHE_VOLUME, actions::TOOL_CACHE),
                    format!("{}:{}", actions::CACHE_VOLUME, actions::CACHE_DIR),
                    format!(
                        "{}:{}",
                        super::artifacts::volume(id),
                        actions::ARTIFACTS_DIR
                    ),
                ]),
                extra_hosts: (!service_hosts.is_empty()).then_some(service_hosts),
                mounts: socket.clone().map(|m| vec![m]),
                ..HostConfig::default()
            }),
            ..ContainerCreateBody::default()
        };
        self.container = match self.create(&name, body.clone()).await {
            Ok(id) => id,
            // The engine has its socket elsewhere: the job runs without it.
            Err(_) if socket.is_some() => {
                if let Some(host) = body.host_config.as_mut() {
                    host.mounts = None;
                }
                self.create(&name, body).await?
            }
            Err(e) => return Err(e),
        };
        // The image's PATH, which GITHUB_PATH adds to.
        if let Ok(inspect) = client.docker.inspect_container(&self.container, None).await
            && let Some(path) = inspect
                .config
                .and_then(|c| c.env)
                .unwrap_or_default()
                .iter()
                .find_map(|e| e.strip_prefix("PATH=").map(str::to_owned))
        {
            self.base_path = path;
        }
        let p = &self.paths;
        let prepare = format!(
            "mkdir -p '{ws}' '{home}' '{tmp}/_runner_file_commands' '{tmp}/_github_workflow' && touch {files} && \
             (id runner >/dev/null 2>&1 && chown -R runner:runner '{ws}' '{tmp}' || true) && \
             {docker_group}",
            docker_group = DOCKER_GROUP,
            ws = p.workspace,
            home = p.home,
            tmp = p.temp,
            files = ["env", "output", "path", "summary", "state"]
                .map(|f| format!("'{}'", p.file(f)))
                .join(" "),
        );
        let (code, _, err) = self.capture(&["sh", "-c", &prepare], "root").await?;
        if code != 0 {
            return Err(format!("Couldn't prepare the job's folders: {err}"));
        }
        let event = serde_json::to_vec_pretty(&data["github"]["event"]).unwrap_or_default();
        self.put_file(
            &format!("{}/_github_workflow", p.temp),
            "event.json",
            &event,
        )
        .await?;
        // Services share the job's network, so they're on localhost.
        for (sid, service) in &self.job.services {
            self.start_service(sid, service, &labels).await?;
        }
        Ok(())
    }

    async fn pull(&self, image: &str) -> Result<(), String> {
        let client = self.run.client;
        if client.has_image(image).await {
            return Ok(());
        }
        self.info(0, format!("Pulling {image}…"));
        client
            .pull(image, |_| {})
            .await
            .map_err(|e| format!("Couldn't pull {image}: {e}"))
    }

    async fn create(&self, name: &str, body: ContainerCreateBody) -> Result<String, String> {
        let client = self.run.client;
        let created = client
            .docker
            .create_container(
                Some(CreateContainerOptionsBuilder::new().name(name).build()),
                body,
            )
            .await
            .map_err(describe)?;
        client
            .docker
            .start_container(&created.id, None::<StartContainerOptions>)
            .await
            .map_err(describe)?;
        Ok(created.id)
    }

    async fn start_service(
        &mut self,
        sid: &str,
        service: &Container,
        labels: &HashMap<String, String>,
    ) -> Result<(), String> {
        let data = self.ctx_data(&[]);
        let hash = |p: &[String]| workspace::hash_files(&self.run.req.project, p);
        let text = |t: &str| {
            expr::interpolate(
                t,
                &Context {
                    data: &data,
                    status: Status::Success,
                    hash_files: Some(&hash),
                },
            )
        };
        let image = text(&service.image)?;
        self.pull(&image).await?;
        let mut env = Vec::new();
        for (k, v) in &service.env {
            env.push(format!("{k}={}", text(v)?));
        }
        let options = text(&service.options)?;
        let health = health_from(&options);
        let body = ContainerCreateBody {
            image: Some(image.clone()),
            env: Some(env),
            labels: Some(labels.clone()),
            healthcheck: health.clone(),
            host_config: Some(HostConfig {
                network_mode: Some(format!("container:{}", self.container)),
                ..HostConfig::default()
            }),
            ..ContainerCreateBody::default()
        };
        let id = self
            .create(
                &format!("ferrodock-ci-{}-{}-{sid}", self.id(), self.idx),
                body,
            )
            .await?;
        self.services.push(id.clone());
        self.info(
            0,
            format!("Service {sid}: {image}, on localhost and as {sid}"),
        );
        if health.is_some() {
            self.info(0, format!("Waiting for {sid} to be healthy…"));
            let deadline = Instant::now() + Duration::from_secs(180);
            loop {
                let state = self
                    .run
                    .client
                    .docker
                    .inspect_container(&id, None)
                    .await
                    .map_err(describe)?
                    .state;
                let health = state
                    .as_ref()
                    .and_then(|s| s.health.as_ref())
                    .and_then(|h| h.status)
                    .map(|s| s.to_string());
                if health.as_deref() == Some("healthy") {
                    break;
                }
                if state.as_ref().is_some_and(|s| s.running == Some(false)) {
                    return Err(format!("The service {sid} stopped"));
                }
                if Instant::now() > deadline || health.as_deref() == Some("unhealthy") {
                    return Err(format!("The service {sid} didn't become healthy"));
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
        Ok(())
    }

    /// Runs a step. Returns its state (what GitHub calls its conclusion).
    async fn step(&mut self, number: usize, step: &Step, name: &str) -> State {
        let step_id = step.id.clone().unwrap_or_else(|| format!("__step{number}"));
        let data = self.ctx_data(&step.env);
        let hash = |p: &[String]| workspace::hash_files(&self.run.req.project, p);
        let ctx = Context {
            data: &data,
            status: self.status,
            hash_files: Some(&hash),
        };
        let run_it = match expr::condition(step.condition.as_deref(), &ctx) {
            Ok(run_it) => run_it,
            Err(e) => {
                self.log(
                    number,
                    vec![LogLine {
                        text: format!("Its if: can't be read: {e}"),
                        kind: LineKind::Error,
                    }],
                );
                self.status = Status::Failure;
                return State::Failure;
            }
        };
        if !run_it {
            self.record(&step_id, "skipped", "skipped", Map::new());
            return State::Skipped;
        }
        self.step_state(number, State::Running);
        let continue_on_error =
            expr::interpolate(&step.continue_on_error, &ctx).is_ok_and(|v| v == "true");
        let with: Vec<(String, String)> = step
            .with
            .iter()
            .map(|(k, v)| {
                let value = expr::interpolate(v, &ctx).unwrap_or_else(|_| v.clone());
                (k.clone(), value)
            })
            .collect();
        let working_dir = match step
            .working_directory
            .as_deref()
            .or(self.job.defaults.working_directory.as_deref())
            .or(self.run.req.workflow.defaults.working_directory.as_deref())
        {
            Some(dir) => {
                let dir = expr::interpolate(dir, &ctx).unwrap_or_else(|_| dir.to_owned());
                if dir.starts_with('/') {
                    dir
                } else {
                    format!("{}/{dir}", self.paths.workspace)
                }
            }
            None => self.paths.workspace.clone(),
        };
        let limit = self.time_left(step.timeout_minutes);
        let work = async {
            match (&step.run, &step.uses) {
                (Some(script), _) => match expr::interpolate(script, &ctx) {
                    Ok(script) => {
                        let shell = step
                            .shell
                            .as_deref()
                            .or(self.job.defaults.shell.as_deref())
                            .or(self.run.req.workflow.defaults.shell.as_deref());
                        let first = script
                            .lines()
                            .find(|l| !l.trim().is_empty())
                            .unwrap_or("")
                            .trim();
                        let shown = Output::new(self.masks()).hide(&format!("Run {first}"));
                        self.log(number, vec![LogLine::info(shown)]);
                        self.script(number, &step_id, &data, &script, shell, &working_dir, &[])
                            .await
                    }
                    Err(e) => Err(format!("The script can't be read: {e}")),
                },
                (None, Some(uses)) => {
                    self.action(number, &step_id, name, &data, uses, &with, &working_dir)
                        .await
                }
                (None, None) => Ok((0, Vec::new())),
            }
        };
        let result: Result<(i64, Vec<(String, String)>), String> =
            match tokio::time::timeout(limit, work).await {
                Ok(result) => result,
                Err(_) => {
                    self.kill_steps().await;
                    Err("The step ran out of time (its timeout-minutes, or the job's).".into())
                }
            };
        let (state, outcome, outputs) = match result {
            Ok((0, outputs)) => (State::Success, "success", outputs),
            Ok((-2, _)) => (State::Skipped, "skipped", Vec::new()),
            Ok((code, outputs)) => {
                self.log(
                    number,
                    vec![LogLine {
                        text: format!("Process completed with exit code {code}."),
                        kind: LineKind::Error,
                    }],
                );
                (State::Failure, "failure", outputs)
            }
            Err(e) => {
                let kind = if cancelled(self.id()) {
                    LineKind::Info
                } else {
                    LineKind::Error
                };
                let text = if cancelled(self.id()) {
                    "The run was cancelled.".to_owned()
                } else {
                    e
                };
                self.log(number, vec![LogLine { text, kind }]);
                (
                    if cancelled(self.id()) {
                        State::Cancelled
                    } else {
                        State::Failure
                    },
                    "failure",
                    Vec::new(),
                )
            }
        };
        let conclusion = if state == State::Failure && continue_on_error {
            "success"
        } else {
            outcome
        };
        self.record(
            &step_id,
            outcome,
            conclusion,
            outputs
                .into_iter()
                .map(|(k, v)| (k, Value::String(v)))
                .collect(),
        );
        match state {
            State::Failure if !continue_on_error => self.status = Status::Failure,
            State::Cancelled => self.status = Status::Cancelled,
            _ => {}
        }
        if state == State::Failure && continue_on_error {
            self.info(number, "It failed, but continue-on-error is set.");
            return State::Success;
        }
        state
    }

    fn record(
        &mut self,
        step_id: &str,
        outcome: &str,
        conclusion: &str,
        outputs: Map<String, Value>,
    ) {
        self.steps.insert(
            step_id.to_owned(),
            json!({"outcome": outcome, "conclusion": conclusion, "outputs": outputs}),
        );
    }

    /// A `uses:` step: one of the built-in actions, or one from GitHub or
    /// the project.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn action(
        &mut self,
        number: usize,
        step_id: &str,
        name: &str,
        data: &Map<String, Value>,
        uses: &str,
        with: &[(String, String)],
        working_dir: &str,
    ) -> Result<(i64, Vec<(String, String)>), String> {
        let inputs = &actions::input_envs(with);
        match actions::builtin(uses) {
            Builtin::Checkout => self
                .checkout(number, inputs)
                .await
                .map(|()| (0, Vec::new())),
            Builtin::Script { main, post } => {
                if let Some(post) = post {
                    let env = self.process_env(data, step_id, inputs);
                    self.posts.push(Post {
                        name: name.to_owned(),
                        run: PostRun::Script(actions::script(post)),
                        env,
                        condition: None,
                    });
                }
                self.script(
                    number,
                    step_id,
                    data,
                    &actions::script(main),
                    Some("bash"),
                    working_dir,
                    inputs,
                )
                .await
            }
            Builtin::UploadArtifact => {
                let result = self
                    .script(
                        number,
                        step_id,
                        data,
                        &actions::script(actions::UPLOAD_ARTIFACT),
                        Some("bash"),
                        working_dir,
                        inputs,
                    )
                    .await;
                if matches!(result, Ok((0, _))) {
                    let name = inputs
                        .iter()
                        .find(|(k, _)| k == "INPUT_NAME")
                        .map_or("artifact", |(_, v)| v.as_str());
                    self.run.emit(CiEvent::Artifact {
                        run: self.id(),
                        name: name.to_owned(),
                    });
                }
                result
            }
            Builtin::Unsupported => {
                self.external(number, step_id, name, data, uses, with, working_dir)
                    .await
            }
        }
    }

    /// `actions/checkout`: the project's files, as a fresh clone has them.
    async fn checkout(&self, number: usize, inputs: &[(String, String)]) -> Result<(), String> {
        let input = |k: &str| {
            inputs
                .iter()
                .find(|(n, _)| n == k)
                .map(|(_, v)| v.clone())
                .filter(|v| !v.is_empty())
        };
        let target = match input("INPUT_PATH") {
            Some(path) => format!("{}/{path}", self.paths.workspace),
            None => self.paths.workspace.clone(),
        };
        if let Some(repo) = input("INPUT_REPOSITORY").filter(|r| *r != self.run.git.repository) {
            // Another repository: cloned from GitHub.
            let branch =
                input("INPUT_REF").map_or_else(String::new, |r| format!("--branch '{r}' "));
            let script =
                format!("git clone --depth 1 {branch}https://github.com/{repo} '{target}'");
            self.info(number, format!("Cloning {repo}"));
            let (code, out, err) = self
                .capture(&["sh", "-c", &script], &self.paths.user)
                .await?;
            self.info(number, format!("{out}{err}").trim().to_owned());
            return if code == 0 {
                Ok(())
            } else {
                Err(format!("Couldn't clone {repo}"))
            };
        }
        let project = self.run.req.project.clone();
        self.info(
            number,
            format!(
                "Copying {} into the workspace (what a clone would have)",
                project.display()
            ),
        );
        let tar = tokio::task::spawn_blocking(move || workspace::project_tar(&project))
            .await
            .map_err(|e| e.to_string())??;
        let (code, _, err) = self.capture(&["mkdir", "-p", &target], "root").await?;
        if code != 0 {
            return Err(err);
        }
        self.run
            .client
            .docker
            .upload_to_container(
                &self.container,
                Some(UploadToContainerOptions {
                    path: target.clone(),
                    ..Default::default()
                }),
                bollard::body_full(tar.into()),
            )
            .await
            .map_err(describe)?;
        let fix = format!(
            "(id runner >/dev/null 2>&1 && chown -R runner:runner '{target}' || true); \
             git config --system --add safe.directory '*' 2>/dev/null || true"
        );
        self.capture(&["sh", "-c", &fix], "root").await?;
        Ok(())
    }

    /// Runs a script with a shell. Returns its exit code and the outputs it
    /// set, after reading back the files it wrote.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn script(
        &mut self,
        number: usize,
        step_id: &str,
        data: &Map<String, Value>,
        script: &str,
        shell: Option<&str>,
        working_dir: &str,
        extra: &[(String, String)],
    ) -> Result<(i64, Vec<(String, String)>), String> {
        // PowerShell runs only .ps1 files; as on GitHub, it stops at the
        // first error and exits with the last program's code.
        let (extension, body) = match shell {
            Some("pwsh") => (
                "ps1",
                format!(
                    "$ErrorActionPreference = 'stop'\n{script}\n\
                     if ((Test-Path -LiteralPath variable:\\LASTEXITCODE)) {{ exit $LASTEXITCODE }}\n"
                ),
            ),
            Some("python") => ("py", script.to_owned()),
            _ => ("sh", script.to_owned()),
        };
        let file = format!("{}/{step_id}-{number}.{extension}", self.paths.temp);
        let (dir, base) = file.rsplit_once('/').unwrap_or((".", "script.sh"));
        self.put_file(dir, base, body.as_bytes()).await?;
        let has_bash = self
            .capture(&["sh", "-c", "command -v bash"], "")
            .await
            .is_ok_and(|(c, _, _)| c == 0);
        let cmd: Vec<String> = match shell {
            Some("bash") | None if has_bash => {
                ["bash", "--noprofile", "--norc", "-eo", "pipefail", &file]
                    .map(str::to_owned)
                    .to_vec()
            }
            Some("bash") => return Err("bash isn't in this container".into()),
            None | Some("sh") => ["sh", "-e", &file].map(str::to_owned).to_vec(),
            Some("python") => ["python3", &file].map(str::to_owned).to_vec(),
            Some("pwsh") => ["pwsh", "-command", &format!(". '{file}'")]
                .map(str::to_owned)
                .to_vec(),
            Some("powershell" | "cmd") => {
                return Err(format!(
                    "{} is Windows only; use pwsh, which runs on Linux too",
                    shell.unwrap_or("")
                ));
            }
            Some(custom) => custom
                .split_whitespace()
                .map(|w| w.replace("{0}", &file))
                .collect(),
        };
        self.command(number, step_id, data, &cmd, working_dir, extra)
            .await
    }

    /// Runs a program in the job's container as a step (or part of one),
    /// with the step's environment. Returns its exit code and outputs.
    pub(super) async fn command(
        &mut self,
        number: usize,
        step_id: &str,
        data: &Map<String, Value>,
        cmd: &[String],
        working_dir: &str,
        extra: &[(String, String)],
    ) -> Result<(i64, Vec<(String, String)>), String> {
        let env = self.process_env(data, step_id, extra);
        let mut output = Output::new(self.masks());
        let user = self.paths.user.clone();
        let code = self
            .exec(number, cmd, env, &user, working_dir, &mut output)
            .await?;
        let outputs = self.read_files(number, &mut output).await;
        Ok((code, outputs))
    }

    /// Reads `GITHUB_ENV`, `GITHUB_OUTPUT` and `GITHUB_PATH` after a step,
    /// and empties them for the next one.
    async fn read_files(&mut self, number: usize, output: &mut Output) -> Vec<(String, String)> {
        let mut outputs: Vec<(String, String)> = std::mem::take(&mut output.outputs);
        for name in ["env", "output", "path"] {
            let path = self.paths.file(name);
            let Ok((_, text, _)) = self
                .capture(
                    &["sh", "-c", &format!("cat '{path}'; : > '{path}'")],
                    "root",
                )
                .await
            else {
                continue;
            };
            match name {
                "path" => {
                    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
                        self.added_paths.push(line.to_owned());
                    }
                }
                _ => match commands::parse_file(&text) {
                    Ok(entries) if name == "env" => {
                        for (k, v) in entries {
                            self.exported.retain(|(e, _)| *e != k);
                            self.exported.push((k, v));
                        }
                    }
                    Ok(entries) => outputs.extend(entries),
                    Err(e) => self.log(
                        number,
                        vec![LogLine {
                            text: format!("GITHUB_{}: {e}", name.to_uppercase()),
                            kind: LineKind::Warning,
                        }],
                    ),
                },
            }
        }
        outputs
    }

    /// Runs `cmd` in the job's container, streaming its output to the
    /// step's log. Returns its exit code.
    /// How long a step may run: its own `timeout-minutes`, within what's
    /// left of the job's.
    fn time_left(&self, step_minutes: Option<f64>) -> Duration {
        let job = self.deadline.map_or(Duration::from_secs(360 * 60), |d| {
            d.saturating_duration_since(Instant::now())
        });
        step_minutes.map_or(job, |m| minutes_to_duration(m).min(job))
    }

    /// Stops whatever a step left running in the job's container (all but
    /// the container's own process).
    async fn kill_steps(&self) {
        let _ = self.capture(&["kill", "-KILL", "-1"], "root").await;
    }

    async fn exec(
        &self,
        number: usize,
        cmd: &[String],
        env: Vec<String>,
        user: &str,
        working_dir: &str,
        output: &mut Output,
    ) -> Result<i64, String> {
        let docker = &self.run.client.docker;
        let exec = docker
            .create_exec(
                &self.container,
                CreateExecOptions::<String> {
                    cmd: Some(cmd.to_vec()),
                    env: Some(env),
                    user: (!user.is_empty()).then(|| user.to_owned()),
                    working_dir: Some(working_dir.to_owned()),
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    ..Default::default()
                },
            )
            .await
            .map_err(describe)?;
        let StartExecResults::Attached {
            output: mut stream, ..
        } = docker.start_exec(&exec.id, None).await.map_err(describe)?
        else {
            return Err("the step didn't attach".into());
        };
        let mut partial = String::new();
        let mut batch = Vec::new();
        let mut last = Instant::now();
        while let Some(chunk) = stream.next().await {
            partial.push_str(&chunk.map_err(describe)?.to_string());
            while let Some(end) = partial.find('\n') {
                let line: String = partial.drain(..=end).collect();
                if let Some(l) = output.line(&line) {
                    batch.push(l);
                }
            }
            if batch.len() >= 200 || last.elapsed() > Duration::from_millis(150) {
                self.log(number, std::mem::take(&mut batch));
                last = Instant::now();
            }
        }
        if !partial.is_empty()
            && let Some(l) = output.line(&partial)
        {
            batch.push(l);
        }
        self.log(number, batch);
        let code = docker
            .inspect_exec(&exec.id)
            .await
            .map_err(describe)?
            .exit_code
            .unwrap_or(-1);
        Ok(code)
    }

    /// Runs a command and collects what it prints.
    pub(super) async fn capture(
        &self,
        cmd: &[&str],
        user: &str,
    ) -> Result<(i64, String, String), String> {
        let docker = &self.run.client.docker;
        let exec = docker
            .create_exec(
                &self.container,
                CreateExecOptions::<String> {
                    cmd: Some(cmd.iter().map(|s| (*s).to_owned()).collect()),
                    user: (!user.is_empty()).then(|| user.to_owned()),
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    ..Default::default()
                },
            )
            .await
            .map_err(describe)?;
        let StartExecResults::Attached { mut output, .. } =
            docker.start_exec(&exec.id, None).await.map_err(describe)?
        else {
            return Err("the command didn't attach".into());
        };
        let (mut out, mut err) = (String::new(), String::new());
        while let Some(chunk) = output.next().await {
            match chunk.map_err(describe)? {
                bollard::container::LogOutput::StdErr { message } => {
                    err.push_str(&String::from_utf8_lossy(&message))
                }
                other => out.push_str(&other.to_string()),
            }
        }
        let code = docker
            .inspect_exec(&exec.id)
            .await
            .map_err(describe)?
            .exit_code
            .unwrap_or(-1);
        Ok((code, out, err))
    }

    /// Writes a file into the container.
    async fn put_file(&self, dir: &str, name: &str, bytes: &[u8]) -> Result<(), String> {
        let mut tar = tar::Builder::new(Vec::new());
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o755);
        // The runner user (1001) can read and replace it.
        header.set_uid(1001);
        header.set_gid(1001);
        header.set_cksum();
        tar.append_data(&mut header, name, bytes)
            .map_err(|e| e.to_string())?;
        let tar = tar.into_inner().map_err(|e| e.to_string())?;
        self.upload(dir, tar).await
    }

    /// Unpacks a tar into a folder of the job's container.
    pub(super) async fn upload(&self, dir: &str, tar: Vec<u8>) -> Result<(), String> {
        self.run
            .client
            .docker
            .upload_to_container(
                &self.container,
                Some(UploadToContainerOptions {
                    path: dir.to_owned(),
                    ..Default::default()
                }),
                bollard::body_full(tar.into()),
            )
            .await
            .map_err(describe)
    }

    /// The `post` scripts of actions, last first, when their condition
    /// holds (by default, when the job succeeded so far).
    async fn post_steps(&mut self) {
        let posts: Vec<_> = std::mem::take(&mut self.posts);
        for Post {
            name,
            run,
            env,
            condition,
        } in posts.into_iter().rev()
        {
            let number = self.next_step;
            self.next_step += 1;
            self.run.emit(CiEvent::Step {
                run: self.id(),
                job: self.idx,
                step: number,
                name: format!("Post {name}"),
            });
            let data = self.ctx_data(&[]);
            let ctx = Context {
                data: &data,
                status: self.status,
                hash_files: None,
            };
            if !expr::condition(condition.as_deref(), &ctx).unwrap_or(false) {
                self.step_state(number, State::Skipped);
                continue;
            }
            self.step_state(number, State::Running);
            let cmd = match run {
                PostRun::Command(cmd) => Ok(cmd),
                PostRun::Script(script) => {
                    let file = format!("{}/post-{number}.sh", self.paths.temp);
                    let (dir, base) = file.rsplit_once('/').unwrap_or((".", "post.sh"));
                    self.put_file(dir, base, script.as_bytes())
                        .await
                        .map(|()| vec!["bash".into(), "-e".into(), file.clone()])
                }
            };
            let state = match cmd {
                Ok(cmd) => {
                    let mut output = Output::new(self.masks());
                    let user = self.paths.user.clone();
                    let workspace = self.paths.workspace.clone();
                    match self
                        .exec(number, &cmd, env, &user, &workspace, &mut output)
                        .await
                    {
                        Ok(0) => State::Success,
                        _ => State::Failure,
                    }
                }
                Err(_) => State::Failure,
            };
            self.step_state(number, state);
        }
    }

    /// The job's `outputs:`, from its steps.
    fn outputs(&self) -> Map<String, Value> {
        let data = self.ctx_data(&[]);
        let hash = |p: &[String]| workspace::hash_files(&self.run.req.project, p);
        let ctx = Context {
            data: &data,
            status: self.status,
            hash_files: Some(&hash),
        };
        self.job
            .outputs
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    Value::String(expr::interpolate(v, &ctx).unwrap_or_default()),
                )
            })
            .collect()
    }

    /// "Complete job": the job's containers are removed.
    async fn clean_up(&mut self) {
        let number = self.next_step;
        self.next_step += 1;
        let run = self.id();
        self.run.emit(CiEvent::Step {
            run,
            job: self.idx,
            step: number,
            name: "Complete job".into(),
        });
        self.step_state(number, State::Running);
        let docker = &self.run.client.docker;
        let force = || {
            Some(RemoveContainerOptions {
                force: true,
                v: true,
                ..Default::default()
            })
        };
        for id in self
            .services
            .iter()
            .chain(std::iter::once(&self.container))
            .filter(|i| !i.is_empty())
        {
            let _ = docker.remove_container(id, force()).await;
        }
        self.info(number, "Cleaned up the job's containers.");
        self.step_state(number, State::Success);
    }
}

/// A health check from a service's `options:` (`--health-cmd …`), so the
/// job waits for it as GitHub does.
fn health_from(options: &str) -> Option<HealthConfig> {
    let words = shell_words(options);
    let value = |flag: &str| {
        words
            .iter()
            .position(|w| w == flag || w.starts_with(&format!("{flag}=")))
            .and_then(|i| {
                words[i]
                    .split_once('=')
                    .map(|(_, v)| v.to_owned())
                    .or_else(|| words.get(i + 1).cloned())
            })
    };
    let duration = |flag: &str| {
        value(flag).and_then(|v| {
            let (n, unit) = v.split_at(
                v.find(|c: char| !c.is_ascii_digit() && c != '.')
                    .unwrap_or(v.len()),
            );
            let n: f64 = n.parse().ok()?;
            let secs = match unit {
                "ms" => n / 1000.0,
                "m" => n * 60.0,
                "h" => n * 3600.0,
                _ => n,
            };
            Some((secs * 1e9) as i64)
        })
    };
    let cmd = value("--health-cmd")?;
    Some(HealthConfig {
        test: Some(vec!["CMD-SHELL".into(), cmd]),
        interval: duration("--health-interval").or(Some(2_000_000_000)),
        timeout: duration("--health-timeout"),
        retries: value("--health-retries").and_then(|r| r.parse().ok()),
        start_period: duration("--health-start-period"),
        ..HealthConfig::default()
    })
}

/// Where Docker listens, on the engine's host and in a job.
const DOCKER_SOCKET: &str = "/var/run/docker.sock";

/// Lets `runner` use the engine's socket: it joins the socket's group,
/// whatever its id. The socket itself, the engine's, is left as it is.
const DOCKER_GROUP: &str = "(! [ -S /var/run/docker.sock ] || ! id runner >/dev/null 2>&1 || \
     { g=$(stat -c %g /var/run/docker.sock); \
       getent group \"$g\" >/dev/null || groupadd -g \"$g\" docker-host; \
       usermod -aG \"$(getent group \"$g\" | cut -d: -f1)\" runner; } || true)";

/// Whether the engine's socket is at [`DOCKER_SOCKET`] on its host:
/// Docker run as root, not Podman or rootless Docker.
fn engine_socket(info: &SystemInfo, version: &SystemVersion) -> bool {
    let rootless = info
        .security_options
        .iter()
        .flatten()
        .any(|o| o.contains("rootless"));
    let podman = version
        .components
        .iter()
        .flatten()
        .any(|c| c.name.to_lowercase().contains("podman"));
    !rootless && !podman
}

/// Splits like a shell: spaces separate, quotes group.
fn shell_words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quote: Option<char> = None;
    let mut started = false;
    for c in text.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => word.push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                started = true;
            }
            (None, c) if c.is_whitespace() => {
                if started || !word.is_empty() {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            (None, c) => word.push(c),
        }
    }
    if started || !word.is_empty() {
        words.push(word);
    }
    words
}

impl DockerClient {
    /// Removes a run's containers and its artifacts' volume is kept.
    async fn remove_labelled(&self, run: &str) -> Result<(), String> {
        let options = ListContainersOptions {
            all: true,
            filters: Some(HashMap::from([(
                "label".to_owned(),
                vec![format!("{CI_LABEL}={run}")],
            )])),
            ..Default::default()
        };
        for c in self
            .docker
            .list_containers(Some(options))
            .await
            .map_err(describe)?
        {
            if let Some(id) = c.id {
                let _ = self
                    .docker
                    .remove_container(
                        &id,
                        Some(RemoveContainerOptions {
                            force: true,
                            v: true,
                            ..Default::default()
                        }),
                    )
                    .await;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_service_health_options() {
        let health = health_from("--health-cmd \"pg_isready -U postgres\" --health-interval 10s --health-timeout=5s --health-retries 5").unwrap();
        assert_eq!(
            health.test.unwrap(),
            ["CMD-SHELL", "pg_isready -U postgres"]
        );
        assert_eq!(health.interval, Some(10_000_000_000));
        assert_eq!(health.timeout, Some(5_000_000_000));
        assert_eq!(health.retries, Some(5));
        assert!(health_from("--cpus 2").is_none());
        assert_eq!(shell_words("a 'b c' \"\" d"), ["a", "b c", "", "d"]);
    }

    #[test]
    fn shares_the_socket_of_docker_only() {
        use bollard::models::SystemVersionComponents as ComponentVersion;
        let docker = SystemVersion::default();
        let podman = SystemVersion {
            components: Some(vec![ComponentVersion {
                name: "Podman Engine".into(),
                ..ComponentVersion::default()
            }]),
            ..SystemVersion::default()
        };
        let rootless = SystemInfo {
            security_options: Some(vec!["name=seccomp".into(), "name=rootless".into()]),
            ..SystemInfo::default()
        };
        assert!(engine_socket(&SystemInfo::default(), &docker));
        assert!(!engine_socket(&SystemInfo::default(), &podman));
        assert!(!engine_socket(&rootless, &docker));
    }

    #[test]
    fn needs_carry_results_and_outputs() {
        let job = Job {
            needs: vec!["build".into()],
            ..Job::default()
        };
        let mut results = HashMap::new();
        let mut outputs = Map::new();
        outputs.insert("version".into(), Value::String("1.0".into()));
        results.insert(
            "build".into(),
            Outcome {
                state: State::Success,
                outputs,
            },
        );
        let needs = needs_context(&job, &results);
        assert_eq!(needs["build"]["result"], "success");
        assert_eq!(needs["build"]["outputs"]["version"], "1.0");
    }
}

#[cfg(test)]
mod live {
    use super::*;

    const WORKFLOW: &str = r#"
name: Live
on: push
env:
  GREETING: hello
jobs:
  build:
    runs-on: ubuntu-latest
    outputs:
      version: ${{ steps.v.outputs.version }}
    steps:
      - uses: actions/checkout@v4
      - name: Files
        run: |
          cat README.md
          whoami
          echo "::group::Details"
          echo "inside group"
          echo "::endgroup::"
      - id: v
        run: echo "version=1.2.$((40+2))" >> "$GITHUB_OUTPUT"
      - run: |
          echo "FROM_ENV=set by step" >> "$GITHUB_ENV"
          mkdir -p "$HOME/bin" && printf '#!/bin/sh\necho custom tool\n' > "$HOME/bin/mytool" && chmod +x "$HOME/bin/mytool"
          echo "$HOME/bin" >> "$GITHUB_PATH"
      - run: |
          test "$FROM_ENV" = "set by step"
          mytool
          echo "$GREETING from ${{ github.repository }} on ${{ github.ref_name }}"
      - name: Allowed to fail
        run: exit 3
        continue-on-error: true
      - name: Skipped
        if: github.event_name == 'pull_request'
        run: echo nope
      - run: mkdir -p out && echo artifact-content > out/a.txt
      - uses: actions/upload-artifact@v4
        with:
          name: out
          path: out/
      - name: Unknown action
        uses: ferrodock-test/no-such-action@v1
        continue-on-error: true
  test:
    needs: build
    runs-on: ubuntu-latest
    strategy:
      matrix:
        n: [1, 2]
    services:
      redis:
        image: redis:7-alpine
        options: --health-cmd "redis-cli ping" --health-interval 1s --health-retries 30
    steps:
      - run: echo "version ${{ needs.build.outputs.version }} matrix ${{ matrix.n }}"
      - uses: actions/download-artifact@v4
        with:
          name: out
          path: got
      - run: cat got/out/a.txt || cat got/a.txt
      - run: timeout 5 bash -c 'echo PING > /dev/tcp/localhost/6379' && getent hosts redis
  fails:
    needs: build
    runs-on: ubuntu-latest
    steps:
      - run: echo "secret is ${{ secrets.TOKEN }}"
      - run: exit 1
      - if: failure()
        run: echo "ran after failure"
  slow:
    runs-on: ubuntu-latest
    steps:
      - timeout-minutes: 0.05
        continue-on-error: true
        run: sleep 60; echo too-late
      - run: echo after-timeout
  mac:
    runs-on: macos-latest
    steps:
      - run: echo never
"#;

    /// Talks to a real daemon: `cargo test -- --ignored`. Needs the runner
    /// image (or the internet to build it) and `redis:7-alpine`.
    #[tokio::test]
    #[ignore]
    async fn live_daemon_runs_a_workflow() {
        let project =
            std::env::temp_dir().join(format!("ferrodock-ci-live-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&project);
        std::fs::create_dir_all(project.join(".github/workflows")).unwrap();
        std::fs::write(project.join("README.md"), "# demo\n").unwrap();
        std::fs::write(project.join(".github/workflows/live.yml"), WORKFLOW).unwrap();
        let workflow = crate::ci::workflow::parse(".github/workflows/live.yml", WORKFLOW).unwrap();
        let client = DockerClient::connect().unwrap();
        let events = Mutex::new(Vec::new());
        let emit = |e: CiEvent| events.lock().unwrap().push(e);
        let state = run_workflow(
            &client,
            RunRequest {
                run: 4242,
                project: project.clone(),
                workflow,
                event: "push".into(),
                inputs: Vec::new(),
                secrets: vec![("TOKEN".into(), "hunter2".into())],
            },
            &emit,
        )
        .await;
        let events = events.into_inner().unwrap();
        let mut names = HashMap::new();
        let mut jobs = HashMap::new();
        let mut steps: HashMap<(usize, usize), (String, State)> = HashMap::new();
        let mut logs: HashMap<usize, Vec<String>> = HashMap::new();
        for e in &events {
            match e {
                CiEvent::Job { job, name, .. } => {
                    names.insert(*job, name.clone());
                }
                CiEvent::JobState { job, state, .. } => {
                    jobs.insert(*job, *state);
                }
                CiEvent::Step {
                    job, step, name, ..
                } => {
                    steps.insert((*job, *step), (name.clone(), State::Queued));
                }
                CiEvent::StepState {
                    job, step, state, ..
                } => {
                    steps.get_mut(&(*job, *step)).unwrap().1 = *state;
                }
                CiEvent::Log { job, lines, .. } => logs
                    .entry(*job)
                    .or_default()
                    .extend(lines.iter().map(|l| l.text.clone())),
                _ => {}
            }
        }
        let mut report = String::new();
        let mut keys: Vec<_> = names.keys().copied().collect();
        keys.sort();
        for j in &keys {
            report.push_str(&format!("JOB {} {:?}\n", names[j], jobs.get(j)));
            let mut s: Vec<_> = steps.iter().filter(|((jj, _), _)| jj == j).collect();
            s.sort_by_key(|((_, n), _)| *n);
            for ((_, n), (name, st)) in s {
                report.push_str(&format!("  {n} {name}: {st:?}\n"));
            }
            for l in logs.get(j).into_iter().flatten() {
                report.push_str(&format!("    | {l}\n"));
            }
        }
        std::fs::write(std::env::temp_dir().join("ferrodock-ci-live.txt"), &report).unwrap();
        let job = |name: &str| keys.iter().find(|j| names[*j] == name).map(|j| jobs[j]);
        assert_eq!(state, State::Failure, "{report}");
        assert_eq!(job("build"), Some(State::Success), "{report}");
        assert_eq!(job("test (1)"), Some(State::Success), "{report}");
        assert_eq!(job("test (2)"), Some(State::Success), "{report}");
        assert_eq!(job("fails"), Some(State::Failure), "{report}");
        assert_eq!(job("mac"), Some(State::Skipped), "{report}");
        assert_eq!(job("slow"), Some(State::Success), "{report}");
        assert!(report.contains("ran out of time"), "{report}");
        assert!(
            report.contains("after-timeout") && !report.contains("| too-late"),
            "{report}"
        );
        assert!(!report.contains("Run set -eo pipefail"), "{report}");
        assert!(report.contains("version 1.2.42 matrix 2"), "{report}");
        assert!(report.contains("custom tool"), "{report}");
        assert!(report.contains("hello from local/"), "{report}");
        assert!(report.contains("artifact-content"), "{report}");
        assert!(
            report.contains("secret is ***") && !report.contains("hunter2"),
            "{report}"
        );
        assert!(report.contains("ran after failure"), "{report}");
        assert!(report.contains("runner"), "{report}");
        // An action that can't be had fails its step, as on GitHub.
        assert!(
            report.contains("ferrodock-test/no-such-action@v1 couldn't be downloaded")
                || report.contains("ferrodock-test/no-such-action@v1 wasn't found"),
            "{report}"
        );

        // The artifact can be saved, then it goes with the run.
        let saved = client.save_artifact(4242, "out", &project).await.unwrap();
        let mut tar = tar::Archive::new(std::fs::File::open(&saved).unwrap());
        let names: Vec<String> = tar
            .entries()
            .unwrap()
            .map(|e| e.unwrap().path().unwrap().display().to_string())
            .collect();
        assert!(names.iter().any(|n| n.ends_with("a.txt")), "{names:?}");
        client.remove_artifacts(4242).await;
        assert!(client.save_artifact(4242, "out", &project).await.is_err());
        let _ = std::fs::remove_dir_all(project);
    }
}
