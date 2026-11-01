//! `strategy.matrix`: the combinations a job runs with, by GitHub's rules
//! for `include` and `exclude`.

use serde_json::{Map, Value};

/// One combination: matrix key to value, in the order they're written.
pub type Combination = Map<String, Value>;

/// The combinations of a matrix (already evaluated, if it was an
/// expression). A job without a matrix runs once, with none.
pub fn expand(matrix: Option<&Value>) -> Result<Vec<Combination>, String> {
    let Some(matrix) = matrix else {
        return Ok(vec![Combination::new()]);
    };
    let Value::Object(map) = matrix else {
        return Err("the matrix isn't a map of lists".into());
    };
    // The cross product of the lists, in the order they're written.
    let mut combos = vec![Combination::new()];
    for (key, values) in map {
        if key == "include" || key == "exclude" {
            continue;
        }
        let Value::Array(values) = values else {
            return Err(format!("matrix.{key} isn't a list"));
        };
        combos = combos
            .into_iter()
            .flat_map(|combo| {
                values.iter().map(move |v| {
                    let mut c = combo.clone();
                    c.insert(key.clone(), v.clone());
                    c
                })
            })
            .collect();
    }
    let has_lists = map.keys().any(|k| k != "include" && k != "exclude");
    if !has_lists {
        combos.clear();
    }
    for exclude in list(map.get("exclude")) {
        combos.retain(|c| !matches(&exclude, c));
    }
    // An include adds its keys to every combination it doesn't clash with
    // on the original keys; one that fits none is a combination of its own.
    let original: Vec<&String> = map
        .keys()
        .filter(|k| *k != "include" && *k != "exclude")
        .collect();
    // Only combinations of the lists are extended, not ones an include
    // added on its own.
    let crossed = combos.len();
    for include in list(map.get("include")) {
        let mut fitted = false;
        for combo in &mut combos[..crossed] {
            let clashes = include
                .iter()
                .any(|(k, v)| original.contains(&k) && combo.get(k).is_some_and(|c| c != v));
            if !clashes {
                for (k, v) in &include {
                    if !original.contains(&k) || !combo.contains_key(k) {
                        combo.insert(k.clone(), v.clone());
                    }
                }
                fitted = true;
            }
        }
        if !fitted {
            combos.push(include);
        }
    }
    if combos.is_empty() {
        return Err("the matrix has no combinations left".into());
    }
    Ok(combos)
}

/// The entries of `include` or `exclude`.
fn list(value: Option<&Value>) -> Vec<Combination> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|i| i.as_object().cloned())
                .collect()
        })
        .unwrap_or_default()
}

/// Whether every key of `pattern` has the same value in `combo`.
fn matches(pattern: &Combination, combo: &Combination) -> bool {
    pattern.iter().all(|(k, v)| combo.get(k) == Some(v))
}

/// How GitHub names a matrix job when it has no `name`: `test (ubuntu, 20)`.
pub fn job_name(id: &str, combo: &Combination) -> String {
    if combo.is_empty() {
        return id.to_owned();
    }
    let values: Vec<String> = combo.values().map(super::expr::to_text).collect();
    format!("{id} ({})", values.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn names(matrix: Value) -> Vec<String> {
        expand(Some(&matrix))
            .unwrap()
            .iter()
            .map(|c| job_name("t", c))
            .collect()
    }

    #[test]
    fn crosses_lists_in_order() {
        assert_eq!(
            names(json!({"os": ["a", "b"], "node": [18, 20]})),
            ["t (a, 18)", "t (a, 20)", "t (b, 18)", "t (b, 20)"]
        );
        assert_eq!(expand(None).unwrap(), vec![Combination::new()]);
    }

    #[test]
    fn excludes_and_includes() {
        assert_eq!(
            names(json!({
                "os": ["a", "b"],
                "node": [18, 20],
                "exclude": [{"os": "b", "node": 18}],
                "include": [{"os": "a", "experimental": true}, {"os": "c", "node": 22}],
            })),
            [
                "t (a, 18, true)",
                "t (a, 20, true)",
                "t (b, 20)",
                "t (c, 22)"
            ]
        );
        assert_eq!(
            names(json!({"include": [{"x": 1}, {"x": 2}]})),
            ["t (1)", "t (2)"]
        );
        assert!(expand(Some(&json!({"os": ["a"], "exclude": [{"os": "a"}]}))).is_err());
        assert!(expand(Some(&json!({"os": "a"}))).is_err());
    }
}
