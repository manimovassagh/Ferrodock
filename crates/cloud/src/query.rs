//! The query protocol (STS, IAM, SNS, RDS): form fields in, XML out.

use crate::protocol::{Fault, Protocol, Reply, xml_escape};
use crate::route::parse_pairs;
use crate::service::Call;

/// A request's fields: the form body and the query string together.
pub struct Params(Vec<(String, String)>);

impl Params {
    pub fn of(call: &Call) -> Self {
        let mut params = Self::parse(&String::from_utf8_lossy(&call.body));
        params.0.extend(call.query.iter().cloned());
        params
    }

    /// The fields of a form body, `A=1&B=2`.
    pub fn parse(form: &str) -> Self {
        Self(parse_pairs(form))
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// A field the operation can't do without.
    pub fn required(&self, name: &str) -> Result<&str, Fault> {
        self.get(name)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| Fault::bad_request("ValidationError", format!("{name} is required")))
    }

    /// A list sent as `Tags.member.1.Key`, `Tags.member.1.Value`, …: for
    /// each index in order, the fields after it.
    pub fn members(&self, prefix: &str) -> Vec<Vec<(&str, &str)>> {
        let start = format!("{prefix}.member.");
        let mut items: Vec<(u32, Vec<(&str, &str)>)> = Vec::new();
        for (key, value) in &self.0 {
            let Some(rest) = key.strip_prefix(&start) else {
                continue;
            };
            let (index, field) = rest.split_once('.').unwrap_or((rest, ""));
            let Ok(index) = index.parse::<u32>() else {
                continue;
            };
            match items.iter_mut().find(|(i, _)| *i == index) {
                Some((_, fields)) => fields.push((field, value)),
                None => items.push((index, vec![(field, value)])),
            }
        }
        items.sort_by_key(|(i, _)| *i);
        items.into_iter().map(|(_, fields)| fields).collect()
    }

    /// A list of `Key`/`Value` pairs, as tags are sent.
    pub fn tags(&self, prefix: &str) -> Vec<(String, String)> {
        self.members(prefix)
            .into_iter()
            .filter_map(|fields| {
                let field = |name: &str| fields.iter().find(|(k, _)| *k == name).map(|(_, v)| *v);
                Some((
                    field("Key")?.to_owned(),
                    field("Value").unwrap_or_default().to_owned(),
                ))
            })
            .collect()
    }

    /// A list of plain values, as `TagKeys.member.1`.
    pub fn values(&self, prefix: &str) -> Vec<String> {
        self.members(prefix)
            .into_iter()
            .filter_map(|fields| fields.first().map(|(_, v)| (*v).to_owned()))
            .collect()
    }
}

/// The reply to `action`, with `result` (XML) inside its result element.
pub fn reply(action: &str, namespace: &str, result: &str, request_id: &str) -> Reply {
    Reply::ok(
        Protocol::Query,
        format!(
            "<{action}Response xmlns=\"{namespace}\"><{action}Result>{result}</{action}Result>\
             <ResponseMetadata><RequestId>{request_id}</RequestId></ResponseMetadata></{action}Response>"
        ),
    )
}

/// The reply to an action that returns nothing.
pub fn empty_reply(action: &str, namespace: &str, request_id: &str) -> Reply {
    Reply::ok(
        Protocol::Query,
        format!(
            "<{action}Response xmlns=\"{namespace}\">\
             <ResponseMetadata><RequestId>{request_id}</RequestId></ResponseMetadata></{action}Response>"
        ),
    )
}

/// `<name>text</name>`, escaped.
pub fn element(name: &str, text: &str) -> String {
    format!("<{name}>{}</{name}>", xml_escape(text))
}

/// Text made safe in a URL, as IAM returns policy documents.
pub fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            byte => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::route::percent_decode;

    #[test]
    fn reads_fields_and_lists() {
        let p = Params::parse(
            "Action=TagRole&RoleName=app&Tags.member.2.Key=team&Tags.member.2.Value=core\
             &Tags.member.1.Key=env&Tags.member.1.Value=dev&TagKeys.member.1=a&TagKeys.member.2=b",
        );
        assert_eq!(p.get("RoleName"), Some("app"));
        assert_eq!(p.required("RoleName").unwrap(), "app");
        assert_eq!(p.required("Missing").unwrap_err().code, "ValidationError");
        assert_eq!(
            p.tags("Tags"),
            [
                ("env".to_owned(), "dev".to_owned()),
                ("team".to_owned(), "core".to_owned())
            ]
        );
        assert_eq!(p.values("TagKeys"), ["a", "b"]);
        assert!(p.tags("Other").is_empty());
    }

    #[test]
    fn wraps_replies() {
        let body = reply(
            "GetRole",
            "https://iam.amazonaws.com/doc/2010-05-08/",
            "<Role/>",
            "id-1",
        )
        .body;
        assert_eq!(
            std::str::from_utf8(&body).unwrap(),
            "<GetRoleResponse xmlns=\"https://iam.amazonaws.com/doc/2010-05-08/\"><GetRoleResult><Role/></GetRoleResult>\
             <ResponseMetadata><RequestId>id-1</RequestId></ResponseMetadata></GetRoleResponse>"
        );
        assert_eq!(element("Path", "/a&b/"), "<Path>/a&amp;b/</Path>");
    }

    #[test]
    fn encodes_documents_for_urls() {
        let doc = r#"{"Version":"2012-10-17","A":["s3:*"]}"#;
        let encoded = percent_encode(doc);
        assert_eq!(
            encoded,
            "%7B%22Version%22%3A%222012-10-17%22%2C%22A%22%3A%5B%22s3%3A%2A%22%5D%7D"
        );
        assert_eq!(percent_decode(&encoded), doc);
    }
}
