//! An action's `action.yml`: its inputs and outputs, and how it runs
//! (JavaScript, composite steps, or a Docker container).

use std::path::Path;

use serde_json::Value;

use super::workflow::{Step, parse_step, scalar, to_json};

#[derive(Debug, Clone, PartialEq)]
pub struct Action {
    pub name: String,
    /// Each input and its default (may hold expressions).
    pub inputs: Vec<(String, Option<String>)>,
    /// Each output and, for a composite action, its value (an expression).
    pub outputs: Vec<(String, String)>,
    pub runs: Runs,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Runs {
    /// `using: node20` (or node12/16/24): scripts run with Node.js.
    Node {
        main: String,
        pre: Option<String>,
        pre_if: Option<String>,
        post: Option<String>,
        post_if: Option<String>,
    },
    Composite(Vec<Step>),
    /// `using: docker`: an image, or `Dockerfile` in the action.
    Docker {
        image: String,
    },
}

/// Reads `action.yml` (or `action.yaml`) in `dir`.
pub fn read(dir: &Path) -> Result<Action, String> {
    let text = ["action.yml", "action.yaml"]
        .iter()
        .find_map(|f| std::fs::read_to_string(dir.join(f)).ok())
        .ok_or("it has no action.yml")?;
    parse(&text)
}

pub fn parse(text: &str) -> Result<Action, String> {
    let yaml: serde_norway::Value = serde_norway::from_str(text)
        .map_err(|e| format!("its action.yml isn't valid YAML: {e}"))?;
    let doc = to_json(&yaml);
    let runs = doc.get("runs").ok_or("its action.yml has no runs")?;
    let text_of = |key: &str| runs.get(key).map(scalar).filter(|s| !s.is_empty());
    let using = text_of("using").unwrap_or_default().to_lowercase();
    let runs = if using.starts_with("node") {
        Runs::Node {
            main: text_of("main").ok_or("its action.yml has no main")?,
            pre: text_of("pre"),
            pre_if: text_of("pre-if"),
            post: text_of("post"),
            post_if: text_of("post-if"),
        }
    } else if using == "composite" {
        let mut steps = Vec::new();
        for (n, step) in runs
            .get("steps")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
        {
            steps.push(parse_step(step).map_err(|e| format!("its step {}: {e}", n + 1))?);
        }
        Runs::Composite(steps)
    } else if using == "docker" {
        Runs::Docker {
            image: text_of("image").unwrap_or_default(),
        }
    } else {
        return Err(format!("it runs with {using:?}, which isn't known"));
    };
    let inputs = doc
        .get("inputs")
        .and_then(Value::as_object)
        .map(|map| {
            map.iter()
                .map(|(k, v)| (k.clone(), v.get("default").map(scalar)))
                .collect()
        })
        .unwrap_or_default();
    let outputs = doc
        .get("outputs")
        .and_then(Value::as_object)
        .map(|map| {
            map.iter()
                .map(|(k, v)| (k.clone(), v.get("value").map(scalar).unwrap_or_default()))
                .collect()
        })
        .unwrap_or_default();
    Ok(Action {
        name: doc.get("name").map(scalar).unwrap_or_default(),
        inputs,
        outputs,
        runs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_node_composite_and_docker_actions() {
        let node = parse(
            "name: Hello\ninputs:\n  who:\n    default: world\n  token:\n    required: true\nruns:\n  using: node20\n  main: dist/index.js\n  post: dist/post.js\n  post-if: success()\n",
        )
        .unwrap();
        assert_eq!(
            node.inputs,
            [("who".into(), Some("world".into())), ("token".into(), None)]
        );
        let Runs::Node {
            main,
            post,
            post_if,
            ..
        } = node.runs
        else {
            panic!("not node");
        };
        assert_eq!(
            (main.as_str(), post.as_deref(), post_if.as_deref()),
            ("dist/index.js", Some("dist/post.js"), Some("success()"))
        );

        let composite = parse(
            "name: Greet\noutputs:\n  said:\n    value: ${{ steps.say.outputs.text }}\nruns:\n  using: composite\n  steps:\n    - id: say\n      shell: bash\n      run: echo text=hi >> $GITHUB_OUTPUT\n    - uses: actions/checkout@v4\n",
        )
        .unwrap();
        assert_eq!(
            composite.outputs,
            [("said".into(), "${{ steps.say.outputs.text }}".into())]
        );
        let Runs::Composite(steps) = composite.runs else {
            panic!("not composite");
        };
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].id.as_deref(), Some("say"));

        let docker = parse("runs:\n  using: docker\n  image: Dockerfile\n").unwrap();
        assert_eq!(
            docker.runs,
            Runs::Docker {
                image: "Dockerfile".into()
            }
        );
        assert!(parse("runs:\n  using: wasm\n").is_err());
    }
}
