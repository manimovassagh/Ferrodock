//! Reading a workflow file: its triggers, jobs and steps. Values that may
//! hold `${{ … }}` are kept as text, to be evaluated when they're used.

use std::path::Path;

use serde_json::Value;

/// A workflow, as `.github/workflows/<file>` describes it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Workflow {
    /// The file, relative to the project, e.g. `.github/workflows/ci.yml`.
    pub file: String,
    pub name: String,
    /// The events it runs on, e.g. `push`, `pull_request`.
    pub on: Vec<String>,
    /// What a manual run (`workflow_dispatch`) asks for.
    pub inputs: Vec<Input>,
    pub env: Vec<(String, String)>,
    pub defaults: Defaults,
    pub jobs: Vec<Job>,
}

/// An input of a manual run.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Input {
    pub name: String,
    pub description: String,
    pub default: String,
    pub required: bool,
    /// `string`, `boolean`, `choice`, `number` or `environment`.
    pub kind: String,
    pub options: Vec<String>,
}

/// `defaults.run`: the shell and folder of `run` steps.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Defaults {
    pub shell: Option<String>,
    pub working_directory: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Job {
    pub id: String,
    /// May hold expressions, e.g. `Test (${{ matrix.os }})`.
    pub name: Option<String>,
    pub runs_on: Vec<String>,
    pub needs: Vec<String>,
    pub condition: Option<String>,
    pub env: Vec<(String, String)>,
    pub defaults: Defaults,
    /// `strategy.matrix`, as written (an object, or an expression).
    pub matrix: Option<Value>,
    pub fail_fast: bool,
    pub max_parallel: Option<usize>,
    /// `container:`: the image the job runs in instead of a runner.
    pub container: Option<Container>,
    pub services: Vec<(String, Container)>,
    pub outputs: Vec<(String, String)>,
    pub continue_on_error: bool,
    pub timeout_minutes: Option<f64>,
    /// A reusable workflow (`uses:` on a job), which isn't run locally.
    pub uses: Option<String>,
    pub steps: Vec<Step>,
}

/// A container a job runs in, or a service next to it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Container {
    pub image: String,
    pub env: Vec<(String, String)>,
    /// `host:container`, as written.
    pub ports: Vec<String>,
    /// Extra `docker create` options, as written (only a few are used).
    pub options: String,
    pub credentials: Option<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Step {
    pub id: Option<String>,
    pub name: Option<String>,
    pub run: Option<String>,
    pub uses: Option<String>,
    pub with: Vec<(String, String)>,
    pub env: Vec<(String, String)>,
    pub condition: Option<String>,
    pub shell: Option<String>,
    pub working_directory: Option<String>,
    /// `true`, `false`, or an expression.
    pub continue_on_error: String,
    pub timeout_minutes: Option<f64>,
}

/// The workflows of a project, by file name; a file that can't be read
/// gives why.
pub fn find_workflows(project: &Path) -> Vec<Result<Workflow, (String, String)>> {
    let dir = project.join(".github").join("workflows");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut files: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "yml" || e == "yaml"))
        .collect();
    files.sort();
    files
        .into_iter()
        .map(|path| {
            let name = path
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            let file = format!(".github/workflows/{name}");
            std::fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|text| parse(&file, &text))
                .map_err(|e| (file, e))
        })
        .collect()
}

/// Reads a workflow file. `file` is its path in the project.
pub fn parse(file: &str, text: &str) -> Result<Workflow, String> {
    let yaml: serde_norway::Value =
        serde_norway::from_str(text).map_err(|e| format!("isn't valid YAML: {e}"))?;
    let doc = to_json(&yaml);
    let Value::Object(top) = &doc else {
        return Err("isn't a workflow (no map at the top)".into());
    };
    let jobs_value = top
        .get("jobs")
        .and_then(Value::as_object)
        .ok_or("has no jobs")?;
    // YAML 1.1 readers turn the key `on` into `true`.
    let on = top
        .get("on")
        .or_else(|| top.get("true"))
        .unwrap_or(&Value::Null);
    let mut jobs = Vec::new();
    for (id, job) in jobs_value {
        jobs.push(parse_job(id, job).map_err(|e| format!("job {id}: {e}"))?);
    }
    Ok(Workflow {
        file: file.to_owned(),
        // Unnamed, GitHub shows the file.
        name: top
            .get("name")
            .map(scalar)
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| file.to_owned()),
        on: events(on),
        inputs: inputs(on),
        env: pairs(top.get("env")),
        defaults: defaults(top.get("defaults")),
        jobs,
    })
}

fn parse_job(id: &str, job: &Value) -> Result<Job, String> {
    let Value::Object(map) = job else {
        return Err("isn't a map".into());
    };
    let strategy = map.get("strategy");
    let mut steps = Vec::new();
    for (n, step) in map
        .get("steps")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        steps.push(parse_step(step).map_err(|e| format!("step {}: {e}", n + 1))?);
    }
    Ok(Job {
        id: id.to_owned(),
        name: map.get("name").map(scalar),
        runs_on: list(map.get("runs-on")),
        needs: list(map.get("needs")),
        condition: map.get("if").map(scalar),
        env: pairs(map.get("env")),
        defaults: defaults(map.get("defaults")),
        matrix: strategy.and_then(|s| s.get("matrix")).cloned(),
        fail_fast: strategy
            .and_then(|s| s.get("fail-fast"))
            .is_none_or(|v| scalar(v) != "false"),
        max_parallel: strategy
            .and_then(|s| s.get("max-parallel"))
            .and_then(|v| scalar(v).parse().ok()),
        container: map.get("container").map(container),
        services: map
            .get("services")
            .and_then(Value::as_object)
            .map(|s| s.iter().map(|(k, v)| (k.clone(), container(v))).collect())
            .unwrap_or_default(),
        outputs: pairs(map.get("outputs")),
        continue_on_error: map
            .get("continue-on-error")
            .is_some_and(|v| scalar(v) == "true"),
        timeout_minutes: map
            .get("timeout-minutes")
            .and_then(|v| scalar(v).parse().ok()),
        uses: map.get("uses").map(scalar),
        steps,
    })
}

fn parse_step(step: &Value) -> Result<Step, String> {
    let Value::Object(map) = step else {
        return Err("isn't a map".into());
    };
    let text = |key: &str| map.get(key).map(scalar);
    let step = Step {
        id: text("id"),
        name: text("name"),
        run: text("run"),
        uses: text("uses"),
        with: pairs(map.get("with")),
        env: pairs(map.get("env")),
        condition: text("if"),
        shell: text("shell"),
        working_directory: text("working-directory"),
        continue_on_error: text("continue-on-error").unwrap_or_else(|| "false".into()),
        timeout_minutes: text("timeout-minutes").and_then(|t| t.parse().ok()),
    };
    if step.run.is_none() && step.uses.is_none() {
        return Err("has neither run nor uses".into());
    }
    Ok(step)
}

fn container(value: &Value) -> Container {
    match value {
        Value::Object(map) => Container {
            image: map.get("image").map(scalar).unwrap_or_default(),
            env: pairs(map.get("env")),
            ports: list(map.get("ports")),
            options: map.get("options").map(scalar).unwrap_or_default(),
            credentials: map.get("credentials").map(|c| {
                (
                    c.get("username").map(scalar).unwrap_or_default(),
                    c.get("password").map(scalar).unwrap_or_default(),
                )
            }),
        },
        other => Container {
            image: scalar(other),
            ..Container::default()
        },
    }
}

fn defaults(value: Option<&Value>) -> Defaults {
    let run = value.and_then(|d| d.get("run"));
    Defaults {
        shell: run.and_then(|r| r.get("shell")).map(scalar),
        working_directory: run.and_then(|r| r.get("working-directory")).map(scalar),
    }
}

/// `on:` as a list of event names, whichever way it's written.
fn events(on: &Value) -> Vec<String> {
    match on {
        Value::Object(map) => map.keys().cloned().collect(),
        other => list(Some(other)),
    }
}

fn inputs(on: &Value) -> Vec<Input> {
    let Some(inputs) = on
        .get("workflow_dispatch")
        .and_then(|d| d.get("inputs"))
        .and_then(Value::as_object)
    else {
        return Vec::new();
    };
    inputs
        .iter()
        .map(|(name, spec)| Input {
            name: name.clone(),
            description: spec.get("description").map(scalar).unwrap_or_default(),
            default: spec.get("default").map(scalar).unwrap_or_default(),
            required: spec.get("required").is_some_and(|r| scalar(r) == "true"),
            kind: spec.get("type").map_or_else(|| "string".to_owned(), scalar),
            options: list(spec.get("options")),
        })
        .collect()
}

/// A map of names to text values (`env:`, `with:`, `outputs:`), in order.
fn pairs(value: Option<&Value>) -> Vec<(String, String)> {
    value
        .and_then(Value::as_object)
        .map(|map| map.iter().map(|(k, v)| (k.clone(), scalar(v))).collect())
        .unwrap_or_default()
}

/// One value or a list of them, as text.
fn list(value: Option<&Value>) -> Vec<String> {
    match value {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(items)) => items.iter().map(scalar).collect(),
        Some(other) => vec![scalar(other)],
    }
}

/// A value as the text a workflow means by it: `true`, `3`, the string.
pub(super) fn scalar(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        other => other.to_string(),
    }
}

/// YAML to JSON, keeping the order of keys.
fn to_json(value: &serde_norway::Value) -> Value {
    use serde_norway::Value as Y;
    match value {
        Y::Null => Value::Null,
        Y::Bool(b) => Value::Bool(*b),
        Y::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::from(i)
            } else if let Some(u) = n.as_u64() {
                Value::from(u)
            } else {
                serde_json::Number::from_f64(n.as_f64().unwrap_or_default())
                    .map_or(Value::Null, Value::Number)
            }
        }
        Y::String(s) => Value::String(s.clone()),
        Y::Sequence(items) => Value::Array(items.iter().map(to_json).collect()),
        Y::Mapping(map) => Value::Object(
            map.iter()
                .map(|(k, v)| {
                    let key = match k {
                        Y::String(s) => s.clone(),
                        Y::Bool(b) => b.to_string(),
                        other => serde_norway::to_string(other)
                            .unwrap_or_default()
                            .trim()
                            .to_owned(),
                    };
                    (key, to_json(v))
                })
                .collect(),
        ),
        Y::Tagged(tagged) => to_json(&tagged.value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CI: &str = r#"
name: CI
on:
  push:
    branches: [main]
  pull_request:
  workflow_dispatch:
    inputs:
      level:
        description: How hard to test
        type: choice
        options: [quick, full]
        default: quick
env:
  CARGO_TERM_COLOR: always
jobs:
  test:
    name: Test (${{ matrix.os }})
    runs-on: ubuntu-latest
    strategy:
      fail-fast: false
      matrix:
        os: [a, b]
    services:
      db:
        image: postgres:17
        env:
          POSTGRES_PASSWORD: pw
    steps:
      - uses: actions/checkout@v4
      - name: Test
        id: test
        run: |
          echo hi
          echo "n=1" >> "$GITHUB_OUTPUT"
        env:
          LEVEL: ${{ inputs.level }}
        continue-on-error: true
  deploy:
    needs: test
    if: github.ref == 'refs/heads/main'
    runs-on: [ubuntu-24.04]
    container: node:22
    steps:
      - run: echo deploy
"#;

    #[test]
    fn reads_a_workflow() {
        let w = parse(".github/workflows/ci.yml", CI).unwrap();
        assert_eq!(w.name, "CI");
        assert_eq!(w.on, ["push", "pull_request", "workflow_dispatch"]);
        assert_eq!(w.inputs[0].name, "level");
        assert_eq!(w.inputs[0].options, ["quick", "full"]);
        assert_eq!(w.env, [("CARGO_TERM_COLOR".into(), "always".into())]);
        let test = &w.jobs[0];
        assert_eq!(test.id, "test");
        assert_eq!(test.runs_on, ["ubuntu-latest"]);
        assert!(!test.fail_fast);
        assert_eq!(test.matrix.as_ref().unwrap()["os"][1], "b");
        assert_eq!(test.services[0].0, "db");
        assert_eq!(test.services[0].1.image, "postgres:17");
        assert_eq!(test.steps[0].uses.as_deref(), Some("actions/checkout@v4"));
        assert!(
            test.steps[1]
                .run
                .as_ref()
                .unwrap()
                .contains("GITHUB_OUTPUT")
        );
        assert_eq!(test.steps[1].continue_on_error, "true");
        let deploy = &w.jobs[1];
        assert_eq!(deploy.needs, ["test"]);
        assert_eq!(deploy.container.as_ref().unwrap().image, "node:22");
        assert_eq!(
            deploy.condition.as_deref(),
            Some("github.ref == 'refs/heads/main'")
        );
    }

    #[test]
    fn reads_short_forms_and_complains_clearly() {
        let w = parse(
            "w.yml",
            "on: push\njobs:\n  a:\n    runs-on: ubuntu-22.04\n    steps:\n      - run: true\n",
        )
        .unwrap();
        assert_eq!(w.on, ["push"]);
        assert_eq!(w.name, "w.yml");
        let w = parse("w.yml", "on: [push, pull_request]\njobs: {}\n").unwrap();
        assert_eq!(w.on.len(), 2);
        assert!(
            parse("w.yml", "on: push\n")
                .unwrap_err()
                .contains("no jobs")
        );
        assert!(
            parse("w.yml", "jobs:\n  a:\n    steps:\n      - name: x\n")
                .unwrap_err()
                .contains("neither run nor uses")
        );
        assert!(parse("w.yml", "jobs: [").is_err());
    }
}
