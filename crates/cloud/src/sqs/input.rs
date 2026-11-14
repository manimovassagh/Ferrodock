//! A request's fields, the same whichever protocol it came in. JSON
//! bodies are read as they are; query-protocol forms
//! (`Attribute.1.Name=…&Attribute.1.Value=…`) are turned into the JSON
//! shape first, so each operation is written once.

use serde_json::{Map, Value, json};

use crate::protocol::{Fault, Protocol};
use crate::route::parse_pairs;
use crate::service::Call;

/// Lists in forms whose JSON name differs, by the form's name.
const RENAMED: &[(&str, &str)] = &[
    ("Attribute", "Attributes"),
    ("AttributeName", "AttributeNames"),
    ("MessageAttribute", "MessageAttributes"),
    ("MessageAttributeName", "MessageAttributeNames"),
    ("MessageSystemAttributeName", "MessageSystemAttributeNames"),
    ("SendMessageBatchRequestEntry", "Entries"),
    ("DeleteMessageBatchRequestEntry", "Entries"),
    ("ChangeMessageVisibilityBatchRequestEntry", "Entries"),
    ("Tag", "Tags"),
    ("TagKey", "TagKeys"),
];

/// Form lists of `Name`/`Value` (or `Key`/`Value`) pairs that are maps in
/// JSON.
const MAPS: &[&str] = &["Attributes", "MessageAttributes", "Tags"];

pub struct Input(Map<String, Value>);

impl Input {
    pub fn of(call: &Call) -> Result<Self, Fault> {
        match call.route.protocol {
            Protocol::Query => {
                let mut pairs = parse_pairs(&String::from_utf8_lossy(&call.body));
                pairs.extend(call.query.iter().cloned());
                Ok(Self(from_form(&pairs)))
            }
            _ if call.body.is_empty() => Ok(Self(Map::new())),
            _ => match serde_json::from_slice(&call.body) {
                Ok(Value::Object(map)) => Ok(Self(map)),
                _ => Err(Fault::bad_request(
                    "InvalidParameterValue",
                    "the request body isn't a JSON object",
                )),
            },
        }
    }

    /// Sets a field the request didn't send.
    pub fn set_default(&mut self, name: &str, value: &str) {
        self.0
            .entry(name.to_owned())
            .or_insert_with(|| Value::String(value.to_owned()));
    }

    pub fn value(&self, name: &str) -> Option<&Value> {
        self.0.get(name)
    }

    /// A text field; numbers and booleans read as their text.
    pub fn text(&self, name: &str) -> Option<String> {
        text(self.0.get(name)?)
    }

    pub fn required(&self, name: &str) -> Result<String, Fault> {
        self.text(name).filter(|v| !v.is_empty()).ok_or_else(|| {
            Fault::bad_request(
                "MissingParameter",
                format!("The request must contain the parameter {name}."),
            )
            .query_code("MissingParameter")
        })
    }

    pub fn number(&self, name: &str) -> Result<Option<u64>, Fault> {
        match self.text(name) {
            None => Ok(None),
            Some(n) => n.parse().map(Some).map_err(|_| invalid(name, &n)),
        }
    }

    /// A map of text, e.g. `Attributes` or `Tags`.
    pub fn map(&self, name: &str) -> Vec<(String, String)> {
        match self.0.get(name) {
            Some(Value::Object(map)) => map
                .iter()
                .filter_map(|(k, v)| Some((k.clone(), text(v)?)))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// A list of text, e.g. `AttributeNames`.
    pub fn list(&self, name: &str) -> Vec<String> {
        match self.0.get(name) {
            Some(Value::Array(items)) => items.iter().filter_map(text).collect(),
            _ => Vec::new(),
        }
    }

    /// A list of objects, e.g. batch `Entries`, each read as its own input.
    pub fn entries(&self, name: &str) -> Vec<Input> {
        match self.0.get(name) {
            Some(Value::Array(items)) => items
                .iter()
                .filter_map(|item| item.as_object().cloned().map(Input))
                .collect(),
            _ => Vec::new(),
        }
    }
}

pub fn invalid(name: &str, value: &str) -> Fault {
    Fault::bad_request(
        "InvalidParameterValue",
        format!("Value {value} for parameter {name} is invalid."),
    )
    .query_code("InvalidParameterValue")
}

fn text(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// `A.1.B=x&A.2.B=y` into `{"A": [{"B": "x"}, {"B": "y"}]}`, then the
/// form's names and pair lists into JSON's.
fn from_form(pairs: &[(String, String)]) -> Map<String, Value> {
    let mut root = Value::Object(Map::new());
    for (key, value) in pairs {
        if key == "Action" || key == "Version" {
            continue;
        }
        let parts: Vec<&str> = key.split('.').collect();
        insert(&mut root, &parts, value);
    }
    match normalise(root) {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

/// Puts `value` at `path`, where a number in the path is a list index
/// (from 1). Lists are built as maps by index first, and turned into
/// arrays in `normalise`.
fn insert(node: &mut Value, path: &[&str], value: &str) {
    let Some((first, rest)) = path.split_first() else {
        return;
    };
    let Value::Object(map) = node else {
        return;
    };
    if rest.is_empty() {
        map.insert((*first).to_owned(), Value::String(value.to_owned()));
        return;
    }
    let child = map
        .entry((*first).to_owned())
        .or_insert_with(|| Value::Object(Map::new()));
    if !child.is_object() {
        *child = Value::Object(Map::new());
    }
    insert(child, rest, value);
}

/// Index-keyed maps into arrays, renamed lists, and pair lists into maps.
fn normalise(value: Value) -> Value {
    let Value::Object(map) = value else {
        return value;
    };
    let indexed = !map.is_empty() && map.keys().all(|k| k.parse::<u32>().is_ok());
    if indexed {
        let mut items: Vec<(u32, Value)> = map
            .into_iter()
            .map(|(k, v)| (k.parse().unwrap_or(0), normalise(v)))
            .collect();
        items.sort_by_key(|(i, _)| *i);
        return Value::Array(items.into_iter().map(|(_, v)| v).collect());
    }
    let mut out = Map::new();
    for (key, value) in map {
        let name = RENAMED
            .iter()
            .find(|(form, _)| *form == key)
            .map_or(key.as_str(), |(_, json)| json)
            .to_owned();
        let mut value = normalise(value);
        if MAPS.contains(&name.as_str())
            && let Value::Array(items) = &value
        {
            value = pairs_to_map(items);
        }
        out.insert(name, value);
    }
    Value::Object(out)
}

/// `[{"Name": "a", "Value": "1"}]` (or `Key`/`Value`) into `{"a": "1"}`.
fn pairs_to_map(items: &[Value]) -> Value {
    let mut map = Map::new();
    for item in items {
        let name = item
            .get("Name")
            .or_else(|| item.get("Key"))
            .and_then(Value::as_str);
        if let Some(name) = name {
            map.insert(
                name.to_owned(),
                item.get("Value").cloned().unwrap_or(json!("")),
            );
        }
    }
    Value::Object(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form(text: &str) -> Value {
        Value::Object(from_form(&parse_pairs(text)))
    }

    #[test]
    fn reads_forms_as_json() {
        let create = form(
            "Action=CreateQueue&QueueName=jobs&Attribute.1.Name=DelaySeconds&Attribute.1.Value=5\
             &Tag.1.Key=team&Tag.1.Value=shop",
        );
        assert_eq!(
            create,
            json!({"QueueName": "jobs", "Attributes": {"DelaySeconds": "5"}, "Tags": {"team": "shop"}})
        );
        let send = form(
            "MessageBody=hi&MessageAttribute.1.Name=kind&MessageAttribute.1.Value.DataType=String\
             &MessageAttribute.1.Value.StringValue=order",
        );
        assert_eq!(
            send["MessageAttributes"],
            json!({"kind": {"DataType": "String", "StringValue": "order"}})
        );
        let batch = form(
            "SendMessageBatchRequestEntry.2.Id=b&SendMessageBatchRequestEntry.2.MessageBody=two\
             &SendMessageBatchRequestEntry.1.Id=a&SendMessageBatchRequestEntry.1.MessageBody=one\
             &AttributeName.1=All",
        );
        assert_eq!(batch["Entries"][0]["Id"], "a");
        assert_eq!(batch["Entries"][1]["MessageBody"], "two");
        assert_eq!(batch["AttributeNames"], json!(["All"]));
    }
}
