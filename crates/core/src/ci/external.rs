//! Actions that aren't built in: from GitHub (`owner/repo@v1`) or from the
//! project (`./path`). JavaScript actions run with the container's Node.js,
//! with their `pre` and `post` scripts; a composite action's steps run in
//! the step that uses it, each in a group of its log.

use serde_json::{Map, Value, json};

use super::expr::{self, Context, Status};
use super::fetch::{self, Source};
use super::metadata::{self, Action, Runs};
use super::runner::{JobRun, Post, PostRun};
use super::workflow::Step;
use super::{LineKind, LogLine, actions, commands, workspace};

/// How deep composite actions may use composite actions.
const MAX_NESTING: usize = 8;

/// A step's exit code and outputs, as the runner keeps them.
type StepResult = Result<(i64, Vec<(String, String)>), String>;

/// Quotes `text` for `sh`.
fn quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// The command that runs a JavaScript file of an action.
fn node(action_dir: &str, file: &str) -> Vec<String> {
    vec!["node".into(), format!("{action_dir}/{file}")]
}

impl JobRun<'_, '_> {
    /// Runs an action from GitHub or the project.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn external(
        &mut self,
        number: usize,
        step_id: &str,
        name: &str,
        data: &Map<String, Value>,
        uses: &str,
        with: &[(String, String)],
        working_dir: &str,
    ) -> StepResult {
        let (host_dir, label) = match Source::parse(uses)? {
            Source::Local(path) => (self.run.req.project.join(&path), format!("_local/{path}")),
            Source::GitHub {
                owner,
                repo,
                path,
                git_ref,
            } => {
                self.info(number, format!("Download action {owner}/{repo}@{git_ref}"));
                let token = self
                    .run
                    .req
                    .secrets
                    .iter()
                    .find(|(k, _)| k == "GITHUB_TOKEN")
                    .map(|(_, v)| v.clone());
                let (o, r, g) = (owner.clone(), repo.clone(), git_ref.clone());
                let dir =
                    tokio::task::spawn_blocking(move || fetch::fetch(&o, &r, &g, token.as_deref()))
                        .await
                        .map_err(|e| e.to_string())??;
                let dir = if path.is_empty() {
                    dir
                } else {
                    dir.join(&path)
                };
                (dir, format!("{owner}/{repo}/{git_ref}/{path}"))
            }
            Source::Image(image) => return Ok(self.not_yet(number, &format!("docker://{image}"))),
        };
        let action = metadata::read(&host_dir).map_err(|e| format!("{uses}: {e}"))?;
        if let Runs::Docker { .. } = action.runs {
            return Ok(self.not_yet(number, &format!("{uses} (a Docker action)")));
        }
        let label: String = label
            .trim_end_matches('/')
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || "/._-".contains(c) {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let target = format!("{}/_actions/{label}", self.paths.temp);
        self.copy_in(&host_dir, &target).await?;

        let mut data = data.clone();
        if let Some(gh) = data.get_mut("github") {
            gh["action_path"] = Value::String(target.clone());
        }
        let values = self.input_values(&action, with, &data);
        match &action.runs {
            Runs::Node {
                main,
                pre,
                pre_if,
                post,
                post_if,
            } => {
                let has_node = self
                    .capture(&["sh", "-c", "command -v node"], "")
                    .await
                    .is_ok_and(|(code, _, _)| code == 0);
                if !has_node {
                    return Err(format!(
                        "{uses} needs Node.js, which this container doesn't have."
                    ));
                }
                let mut extra = actions::input_envs(&values);
                extra.push(("GITHUB_ACTION_PATH".into(), target.clone()));
                if let Some(pre) = pre
                    && self.holds(pre_if.as_deref().unwrap_or("always()"), &data)
                {
                    let (code, _) = self
                        .command(
                            number,
                            step_id,
                            &data,
                            &node(&target, pre),
                            working_dir,
                            &extra,
                        )
                        .await?;
                    if code != 0 {
                        return Ok((code, Vec::new()));
                    }
                }
                let result = self
                    .command(
                        number,
                        step_id,
                        &data,
                        &node(&target, main),
                        working_dir,
                        &extra,
                    )
                    .await;
                // What the action saved for its post script (GITHUB_STATE).
                for (k, v) in self.take_state().await {
                    extra.push((format!("STATE_{k}"), v));
                }
                if let Some(post) = post {
                    let env = self.process_env(&data, step_id, &extra);
                    self.posts.push(Post {
                        name: name.to_owned(),
                        run: PostRun::Command(node(&target, post)),
                        env,
                        condition: Some(post_if.clone().unwrap_or_else(|| "always()".into())),
                    });
                }
                result
            }
            Runs::Composite(steps) => {
                self.composite(number, step_id, &data, &target, &action, &values, steps)
                    .await
            }
            Runs::Docker { .. } => unreachable!("handled above"),
        }
    }

    /// Logs that an action can't run here yet; the step is skipped.
    fn not_yet(&self, number: usize, what: &str) -> (i64, Vec<(String, String)>) {
        self.log(
            number,
            vec![LogLine {
                text: format!("Local CI can't run {what} yet, so this step is skipped."),
                kind: LineKind::Warning,
            }],
        );
        (-2, Vec::new())
    }

    /// The action's inputs: those given in `with:`, and the defaults of
    /// the rest (which may be expressions, like `${{ github.token }}`).
    fn input_values(
        &self,
        action: &Action,
        with: &[(String, String)],
        data: &Map<String, Value>,
    ) -> Vec<(String, String)> {
        let mut values: Vec<(String, String)> = with.to_vec();
        let hash = |p: &[String]| workspace::hash_files(&self.run.req.project, p);
        let ctx = Context {
            data,
            status: self.status,
            hash_files: Some(&hash),
        };
        for (input, default) in &action.inputs {
            let given = values.iter().any(|(k, _)| k.eq_ignore_ascii_case(input));
            if let (false, Some(default)) = (given, default) {
                let value = expr::interpolate(default, &ctx).unwrap_or_else(|_| default.clone());
                values.push((input.clone(), value));
            }
        }
        values
    }

    fn holds(&self, condition: &str, data: &Map<String, Value>) -> bool {
        let ctx = Context {
            data,
            status: self.status,
            hash_files: None,
        };
        expr::condition(Some(condition), &ctx).unwrap_or(false)
    }

    /// Copies an action's folder into the job's container.
    async fn copy_in(&self, host_dir: &std::path::Path, target: &str) -> Result<(), String> {
        let tar = super::super::build::folder_tar(
            host_dir,
            &super::super::build::Ignore::parse(""),
            &|_| true,
        )?;
        let (code, _, err) = self
            .capture(
                &["sh", "-c", &format!("mkdir -p {}", quoted(target))],
                "root",
            )
            .await?;
        if code != 0 {
            return Err(format!("The action couldn't be copied in: {err}"));
        }
        self.upload(target, tar).await
    }

    /// Reads and empties `GITHUB_STATE`.
    async fn take_state(&self) -> Vec<(String, String)> {
        let path = self.paths.file("state");
        let Ok((_, text, _)) = self
            .capture(
                &[
                    "sh",
                    "-c",
                    &format!("cat '{path}' 2>/dev/null; : > '{path}'"),
                ],
                "root",
            )
            .await
        else {
            return Vec::new();
        };
        commands::parse_file(&text).unwrap_or_default()
    }

    /// Runs a composite action's steps in the step that uses it. Each gets
    /// a group in the log; their outputs make the action's.
    #[allow(clippy::too_many_arguments)]
    async fn composite(
        &mut self,
        number: usize,
        step_id: &str,
        data: &Map<String, Value>,
        target: &str,
        action: &Action,
        values: &[(String, String)],
        steps: &[Step],
    ) -> StepResult {
        if self.nesting >= MAX_NESTING {
            return Err("Composite actions are nested too deep here.".into());
        }
        self.nesting += 1;
        let inputs: Map<String, Value> = values
            .iter()
            .map(|(k, v)| (k.clone(), Value::String(v.clone())))
            .collect();
        let mut own_steps = Map::new();
        let mut failed = false;
        let mut result = Ok(());
        for (k, step) in steps.iter().enumerate() {
            let sub_id = step
                .id
                .clone()
                .unwrap_or_else(|| format!("__{step_id}_{k}"));
            let mut d = data.clone();
            d.insert("inputs".into(), Value::Object(inputs.clone()));
            d.insert("steps".into(), Value::Object(own_steps.clone()));
            let status = if failed { Status::Failure } else { self.status };
            let hash = |p: &[String]| workspace::hash_files(&self.run.req.project, p);
            // The step's env, over the one of the step using the action.
            let mut env = d
                .get("env")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            for (key, value) in &step.env {
                d.insert("env".into(), Value::Object(env.clone()));
                let ctx = Context {
                    data: &d,
                    status,
                    hash_files: Some(&hash),
                };
                let value = expr::interpolate(value, &ctx).unwrap_or_else(|_| value.clone());
                env.insert(key.clone(), Value::String(value));
            }
            d.insert("env".into(), Value::Object(env));
            let ctx = Context {
                data: &d,
                status,
                hash_files: Some(&hash),
            };
            let run_it = match expr::condition(step.condition.as_deref(), &ctx) {
                Ok(run_it) => run_it,
                Err(e) => {
                    result = Err(format!("A step's if: can't be read: {e}"));
                    break;
                }
            };
            if !run_it {
                own_steps.insert(
                    sub_id,
                    json!({"outcome": "skipped", "conclusion": "skipped", "outputs": {}}),
                );
                continue;
            }
            let continue_on_error =
                expr::interpolate(&step.continue_on_error, &ctx).is_ok_and(|v| v == "true");
            let with: Vec<(String, String)> = step
                .with
                .iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        expr::interpolate(v, &ctx).unwrap_or_else(|_| v.clone()),
                    )
                })
                .collect();
            let working_dir = match &step.working_directory {
                Some(dir) => {
                    let dir = expr::interpolate(dir, &ctx).unwrap_or_else(|_| dir.clone());
                    if dir.starts_with('/') {
                        dir
                    } else {
                        format!("{}/{dir}", self.paths.workspace)
                    }
                }
                None => self.paths.workspace.clone(),
            };
            let script = step
                .run
                .as_ref()
                .map(|script| expr::interpolate(script, &ctx));
            let title = step.name.as_ref().map_or_else(
                || match (&step.run, &step.uses) {
                    (Some(run), _) => format!(
                        "Run {}",
                        run.lines()
                            .find(|l| !l.trim().is_empty())
                            .unwrap_or("")
                            .trim()
                    ),
                    (None, Some(uses)) => format!("Run {uses}"),
                    _ => "Run".into(),
                },
                |n| expr::interpolate(n, &ctx).unwrap_or_else(|_| n.clone()),
            );
            self.log(
                number,
                vec![LogLine {
                    text: title.clone(),
                    kind: LineKind::Group,
                }],
            );
            let outcome: StepResult = match (script, &step.uses) {
                (Some(Ok(script)), _) => {
                    let extra = [("GITHUB_ACTION_PATH".to_owned(), target.to_owned())];
                    // A composite action's run step names its shell.
                    let shell = step.shell.as_deref().or(Some("bash"));
                    self.script(
                        number,
                        &format!("{step_id}-{k}"),
                        &d,
                        &script,
                        shell,
                        &working_dir,
                        &extra,
                    )
                    .await
                }
                (Some(Err(e)), _) => Err(format!("The script can't be read: {e}")),
                (None, Some(uses)) => {
                    Box::pin(self.action(number, &sub_id, &title, &d, uses, &with, &working_dir))
                        .await
                }
                (None, None) => Ok((0, Vec::new())),
            };
            self.log(
                number,
                vec![LogLine {
                    text: String::new(),
                    kind: LineKind::EndGroup,
                }],
            );
            let (state, outputs) = match outcome {
                Ok((0, outputs)) => ("success", outputs),
                Ok((-2, _)) => ("skipped", Vec::new()),
                Ok((code, outputs)) => {
                    self.log(
                        number,
                        vec![LogLine {
                            text: format!("{title}: exit code {code}."),
                            kind: LineKind::Error,
                        }],
                    );
                    ("failure", outputs)
                }
                Err(e) => {
                    self.log(
                        number,
                        vec![LogLine {
                            text: e,
                            kind: LineKind::Error,
                        }],
                    );
                    ("failure", Vec::new())
                }
            };
            if state == "failure" && !continue_on_error {
                failed = true;
            }
            let conclusion = if state == "failure" && continue_on_error {
                "success"
            } else {
                state
            };
            let outputs: Map<String, Value> = outputs
                .into_iter()
                .map(|(k, v)| (k, Value::String(v)))
                .collect();
            own_steps.insert(
                sub_id,
                json!({"outcome": state, "conclusion": conclusion, "outputs": outputs}),
            );
        }
        self.nesting -= 1;
        result?;
        // The action's outputs, from its steps.
        let mut d = data.clone();
        d.insert("inputs".into(), Value::Object(inputs));
        d.insert("steps".into(), Value::Object(own_steps));
        let ctx = Context {
            data: &d,
            status: self.status,
            hash_files: None,
        };
        let outputs = action
            .outputs
            .iter()
            .map(|(k, v)| (k.clone(), expr::interpolate(v, &ctx).unwrap_or_default()))
            .collect();
        if failed {
            return Err("A step of the action failed.".into());
        }
        Ok((0, outputs))
    }
}

#[cfg(test)]
mod live {
    use std::sync::Mutex;

    use crate::DockerClient;
    use crate::ci::{CiEvent, RunRequest, State, run_workflow, workflow};

    const WORKFLOW: &str = r#"
name: Actions
on: push
jobs:
  actions:
    runs-on: ubuntu-latest
    steps:
      - id: greet
        uses: ./.github/actions/greet
        with:
          who: Ferrodock
      - run: echo "composite said ${{ steps.greet.outputs.said }}"
      - id: js
        uses: ./.github/actions/js
        with:
          name: local-js
      - run: echo "js gave ${{ steps.js.outputs.n }}"
      - id: hello
        uses: actions/hello-world-javascript-action@main
        with:
          who-to-greet: Mona
      - run: echo "hello at ${{ steps.hello.outputs.time }}"
      - id: script
        uses: actions/github-script@v7
        with:
          script: |
            console.log('sum ' + (1 + 2) + ' in ' + context.repo.repo);
            core.setOutput('r', 'from-script');
      - run: echo "script gave ${{ steps.script.outputs.r }}"
      - uses: ./.github/actions/fails
        continue-on-error: true
      - uses: docker://alpine:3
"#;

    const GREET: &str = r#"
name: Greet
inputs:
  who:
    required: true
  greeting:
    default: Hello
outputs:
  said:
    value: ${{ steps.say.outputs.text }}
runs:
  using: composite
  steps:
    - id: say
      shell: bash
      run: |
        echo "${{ inputs.greeting }}, ${{ inputs.who }} from $GITHUB_ACTION_PATH"
        echo "text=${{ inputs.greeting }} ${{ inputs.who }}" >> "$GITHUB_OUTPUT"
    - uses: ./.github/actions/shout
      with:
        word: nested
    - if: failure()
      shell: bash
      run: echo never-on-success
"#;

    const SHOUT: &str = "runs:\n  using: composite\n  steps:\n    - shell: sh\n      run: echo \"SHOUT ${{ inputs.word }}\"\n";

    const JS: &str = "inputs:\n  name: {}\n  extra:\n    default: from-default\nruns:\n  using: node20\n  pre: pre.js\n  main: main.js\n  post: post.js\n";

    const FAILS: &str = "runs:\n  using: composite\n  steps:\n    - shell: bash\n      run: exit 3\n    - shell: bash\n      run: echo after-failure-never\n";

    /// Talks to a real daemon: `cargo test -- --ignored`. Needs the runner
    /// image, and actions/hello-world-javascript-action@main and
    /// actions/github-script@v7 (downloaded, or already in Ferrodock's
    /// action cache).
    #[tokio::test]
    #[ignore]
    async fn live_daemon_runs_actions() {
        let project =
            std::env::temp_dir().join(format!("ferrodock-ci-actions-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&project);
        let files = [
            (".github/workflows/actions.yml", WORKFLOW),
            (".github/actions/greet/action.yml", GREET),
            (".github/actions/shout/action.yml", SHOUT),
            (".github/actions/fails/action.yml", FAILS),
            (".github/actions/js/action.yml", JS),
            (".github/actions/js/pre.js", "console.log('pre ran');\n"),
            (
                ".github/actions/js/main.js",
                "const fs = require('fs');\nconsole.log('main ' + process.env.INPUT_NAME + ' ' + process.env.INPUT_EXTRA);\nfs.appendFileSync(process.env.GITHUB_OUTPUT, 'n=42\\n');\nfs.appendFileSync(process.env.GITHUB_STATE, 'k=kept\\n');\n",
            ),
            (
                ".github/actions/js/post.js",
                "console.log('post saw ' + process.env.STATE_k);\n",
            ),
        ];
        for (path, text) in files {
            let file = project.join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, text).unwrap();
        }
        let workflow = workflow::parse(".github/workflows/actions.yml", WORKFLOW).unwrap();
        let client = DockerClient::connect().unwrap();
        let events = Mutex::new(Vec::new());
        let emit = |e: CiEvent| events.lock().unwrap().push(e);
        let state = run_workflow(
            &client,
            RunRequest {
                run: 4343,
                project: project.clone(),
                workflow,
                event: "push".into(),
                inputs: Vec::new(),
                secrets: vec![("GITHUB_TOKEN".into(), "local-token".into())],
            },
            &emit,
        )
        .await;
        let mut report = String::new();
        let mut steps = std::collections::BTreeMap::new();
        for e in events.into_inner().unwrap() {
            match e {
                CiEvent::Step { step, name, .. } => {
                    steps.insert(step, (name, State::Queued));
                }
                CiEvent::StepState { step, state, .. } => {
                    if let Some(s) = steps.get_mut(&step) {
                        s.1 = state;
                    }
                }
                CiEvent::Log { step, lines, .. } => {
                    for l in lines {
                        report.push_str(&format!("{step} {:?} {}\n", l.kind, l.text));
                    }
                }
                _ => {}
            }
        }
        for (n, (name, state)) in &steps {
            report.push_str(&format!("STEP {n} {name}: {state:?}\n"));
        }
        std::fs::write(
            std::env::temp_dir().join("ferrodock-ci-actions.txt"),
            &report,
        )
        .unwrap();
        let _ = std::fs::remove_dir_all(&project);
        assert_eq!(state, State::Success, "{report}");
        for wanted in [
            "Hello, Ferrodock from /",
            "SHOUT nested",
            "composite said Hello Ferrodock",
            "pre ran",
            "main local-js from-default",
            "js gave 42",
            "Hello, Mona!",
            "sum 3 in ferrodock-ci-actions-",
            "script gave from-script",
            "post saw kept",
            "can't run docker://alpine:3 yet",
            "A step of the action failed.",
        ] {
            assert!(report.contains(wanted), "missing {wanted:?}\n{report}");
        }
        assert!(
            !report.contains("never-on-success") || report.contains("Run echo never"),
            "{report}"
        );
        assert!(!report.contains("\nafter-failure-never"), "{report}");
        assert!(!report.contains("hello at \n"), "{report}");
    }
}
