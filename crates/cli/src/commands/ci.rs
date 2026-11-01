//! `ci list` and `ci run`: Local CI on the command line. A run prints each
//! step's log as it comes, with the job it's in, and ends with a summary.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ferrodock_core::ci::{
    CiEvent, LineKind, RunRequest, State, Workflow, cancel_run, find_workflows, run_workflow,
    secrets,
};
use serde::Serialize;

use super::Ctx;
use crate::output;

/// The project's folder, made absolute (its name and `.git` matter).
fn project_dir(project: &Path) -> Result<PathBuf, String> {
    let dir = std::fs::canonicalize(project).map_err(|e| format!("{}: {e}", project.display()))?;
    if !dir.join(".github").join("workflows").is_dir() {
        return Err(format!("{} has no .github/workflows", dir.display()));
    }
    Ok(dir)
}

#[derive(Serialize)]
struct Listed {
    file: String,
    name: String,
    on: Vec<String>,
    jobs: Vec<String>,
}

pub fn list(project: &Path, json: bool) -> Result<(), String> {
    let dir = project_dir(project)?;
    let mut listed = Vec::new();
    let mut rows = Vec::new();
    for found in find_workflows(&dir) {
        match found {
            Ok(w) => {
                let jobs: Vec<String> = w.jobs.iter().map(|j| j.id.clone()).collect();
                rows.push(vec![
                    w.file.trim_start_matches(".github/workflows/").to_owned(),
                    w.name.clone(),
                    w.on.join(", "),
                    jobs.join(", "),
                ]);
                listed.push(Listed {
                    file: w.file,
                    name: w.name,
                    on: w.on,
                    jobs,
                });
            }
            Err((file, why)) => eprintln!("{file}: {why}"),
        }
    }
    if json {
        return output::json(&listed);
    }
    output::print_table(
        &["FILE", "NAME", "ON", "JOBS"],
        &rows,
        "There are no workflows in .github/workflows.",
    );
    Ok(())
}

/// The workflow `wanted` names (its file, with or without the folder, or
/// its name), or the only one.
fn pick(dir: &Path, wanted: Option<&str>) -> Result<Workflow, String> {
    let all: Vec<Workflow> = find_workflows(dir)
        .into_iter()
        .filter_map(Result::ok)
        .collect();
    let names = || {
        all.iter()
            .map(|w| w.file.trim_start_matches(".github/workflows/").to_owned())
            .collect::<Vec<_>>()
            .join(", ")
    };
    match wanted {
        None if all.len() == 1 => Ok(all[0].clone()),
        None => Err(format!("Which workflow? There are: {}", names())),
        Some(wanted) => all
            .iter()
            .find(|w| {
                w.file == wanted
                    || w.file.trim_start_matches(".github/workflows/") == wanted
                    || w.name.eq_ignore_ascii_case(wanted)
            })
            .cloned()
            .ok_or_else(|| format!("There's no workflow {wanted}. There are: {}", names())),
    }
}

/// Secrets saved in Ferrodock for the project, then the ones given:
/// `NAME=VALUE`, or `NAME` for the environment's value.
fn secrets_for(dir: &Path, given: &[String]) -> Result<Vec<(String, String)>, String> {
    let mut all = secrets::load(dir);
    for item in given {
        let (name, value) = match item.split_once('=') {
            Some((n, v)) => (n.to_owned(), v.to_owned()),
            None => {
                let value = std::env::var(item).map_err(|_| {
                    format!("--secret {item}: there's no {item} in the environment")
                })?;
                (item.clone(), value)
            }
        };
        all.retain(|(k, _)| *k != name);
        all.push((name, value));
    }
    Ok(all)
}

#[derive(Serialize, Default, Clone)]
struct StepSummary {
    name: String,
    state: &'static str,
}

#[derive(Serialize, Default, Clone)]
struct JobSummary {
    name: String,
    state: &'static str,
    steps: Vec<StepSummary>,
    /// The error lines of its log.
    errors: Vec<String>,
}

#[derive(Serialize)]
struct RunSummary {
    workflow: String,
    state: &'static str,
    jobs: Vec<JobSummary>,
    /// Artifacts saved with --artifacts, as files.
    artifacts: Vec<String>,
}

fn state_text(state: State) -> &'static str {
    match state {
        State::Queued => "queued",
        State::Running => "running",
        State::Success => "success",
        State::Failure => "failure",
        State::Skipped => "skipped",
        State::Cancelled => "cancelled",
    }
}

/// What the run has said so far, and printing it as it comes.
struct Printer {
    json: bool,
    jobs: HashMap<usize, JobSummary>,
    artifacts: Vec<String>,
}

impl Printer {
    fn event(&mut self, event: CiEvent) {
        let mut out = std::io::stdout().lock();
        match event {
            CiEvent::Job { job, name, .. } => {
                self.jobs.entry(job).or_default().name = name;
            }
            CiEvent::JobState { job, state, .. } => {
                let j = self.jobs.entry(job).or_default();
                j.state = state_text(state);
                if !self.json && state.done() {
                    let _ = writeln!(out, "● {} {}", j.name, state.label().to_lowercase());
                }
            }
            CiEvent::Step {
                job, step, name, ..
            } => {
                let j = self.jobs.entry(job).or_default();
                if j.steps.len() <= step {
                    j.steps.resize(step + 1, StepSummary::default());
                }
                j.steps[step].name = name;
            }
            CiEvent::StepState {
                job, step, state, ..
            } => {
                let j = self.jobs.entry(job).or_default();
                if j.steps.len() <= step {
                    j.steps.resize(step + 1, StepSummary::default());
                }
                j.steps[step].state = state_text(state);
                if !self.json && state == State::Running {
                    let _ = writeln!(out, "▶ {} › {}", j.name, j.steps[step].name);
                }
            }
            CiEvent::Log { job, lines, .. } => {
                let j = self.jobs.entry(job).or_default();
                for line in lines {
                    if line.kind == LineKind::Error {
                        j.errors.push(line.text.clone());
                    }
                    if self.json {
                        continue;
                    }
                    let text = match line.kind {
                        LineKind::Group => format!("▸ {}", line.text),
                        LineKind::EndGroup => continue,
                        LineKind::Error => format!("Error: {}", line.text),
                        LineKind::Warning => format!("Warning: {}", line.text),
                        LineKind::Notice => format!("Notice: {}", line.text),
                        LineKind::Output | LineKind::Info => line.text,
                    };
                    let _ = writeln!(out, "  {} │ {text}", j.name);
                }
            }
            CiEvent::Artifact { name, .. } => {
                if !self.artifacts.contains(&name) {
                    self.artifacts.push(name);
                }
            }
            CiEvent::Done { .. } => {}
        }
        let _ = out.flush();
    }
}

pub async fn run(
    ctx: &Ctx,
    project: &Path,
    workflow: Option<&str>,
    event: Option<String>,
    inputs: Vec<(String, String)>,
    given_secrets: &[String],
    artifacts_dir: Option<&Path>,
) -> Result<(), String> {
    let dir = project_dir(project)?;
    let workflow = pick(&dir, workflow)?;
    let event = event.unwrap_or_else(|| {
        ["push", "pull_request", "workflow_dispatch"]
            .into_iter()
            .find(|e| workflow.on.iter().any(|o| o == e))
            .unwrap_or("push")
            .to_owned()
    });
    // Defaults of the manual run's inputs, then the ones given.
    let mut all_inputs: Vec<(String, String)> = workflow
        .inputs
        .iter()
        .map(|i| (i.name.clone(), i.default.clone()))
        .collect();
    for (k, v) in inputs {
        all_inputs.retain(|(n, _)| *n != k);
        all_inputs.push((k, v));
    }
    let run = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |d| d.as_millis() as u64);
    let name = workflow.name.clone();
    if !ctx.json {
        println!("Running {name} ({}) as {event}", workflow.file);
    }
    let secrets = secrets_for(&dir, given_secrets)?;
    let request = RunRequest {
        run,
        project: dir,
        workflow,
        event,
        inputs: all_inputs,
        secrets,
    };
    let printer = Mutex::new(Printer {
        json: ctx.json,
        jobs: HashMap::new(),
        artifacts: Vec::new(),
    });
    let emit = |e: CiEvent| {
        if let Ok(mut p) = printer.lock() {
            p.event(e);
        }
    };
    let client = &ctx.conn.client;
    let state = tokio::select! {
        state = run_workflow(client, request, &emit) => state,
        _ = tokio::signal::ctrl_c() => {
            eprintln!("Stopping the run…");
            cancel_run(client, run).await;
            State::Cancelled
        }
    };
    let printer = printer.into_inner().map_err(|e| e.to_string())?;
    let mut saved = Vec::new();
    if let Some(to) = artifacts_dir {
        std::fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
        for artifact in &printer.artifacts {
            match client.save_artifact(run, artifact, to).await {
                Ok(file) => saved.push(file.display().to_string()),
                Err(e) => eprintln!("Couldn't save the artifact {artifact}: {e}"),
            }
        }
    }
    client.remove_artifacts(run).await;
    let mut jobs: Vec<(usize, JobSummary)> = printer.jobs.into_iter().collect();
    jobs.sort_by_key(|(i, _)| *i);
    let jobs: Vec<JobSummary> = jobs
        .into_iter()
        .map(|(_, j)| j)
        .filter(|j| !j.name.is_empty())
        .collect();
    if ctx.json {
        output::json(&RunSummary {
            workflow: name.clone(),
            state: state_text(state),
            jobs,
            artifacts: saved,
        })?;
    } else {
        let rows: Vec<Vec<String>> = jobs
            .iter()
            .map(|j| vec![j.name.clone(), j.state.to_owned()])
            .collect();
        println!();
        output::print_table(&["JOB", "RESULT"], &rows, "No jobs ran.");
        for file in &saved {
            println!("Saved {file}");
        }
        if artifacts_dir.is_none() && !printer.artifacts.is_empty() {
            println!(
                "Artifacts ({}) weren't kept; --artifacts DIR saves them.",
                printer.artifacts.join(", ")
            );
        }
    }
    match state {
        State::Success => Ok(()),
        other => Err(format!("{name} {}", other.label().to_lowercase())),
    }
}
