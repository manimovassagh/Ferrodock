//! Replies in the query protocol: the same results the JSON protocol
//! gives, written as SQS's XML. Lists are flat (one element per item, no
//! wrapper), and maps are lists of name and value elements.

use serde_json::Value;

use crate::protocol::{Protocol, Reply, xml_escape};

const NAMESPACE: &str = "http://queue.amazonaws.com/doc/2012-11-05/";

/// The element each list's items are written as, by the list's JSON name.
fn item_name(list: &str, action: &str) -> String {
    match list {
        "QueueUrls" | "queueUrls" => "QueueUrl".into(),
        "Messages" => "Message".into(),
        "Failed" => "BatchResultErrorEntry".into(),
        "Successful" => format!("{action}ResultEntry"),
        other => other.trim_end_matches('s').to_owned(),
    }
}

/// How a map is written: its item element and the key's element name.
fn map_shape(map: &str) -> Option<(&'static str, &'static str)> {
    match map {
        "Attributes" => Some(("Attribute", "Name")),
        "MessageAttributes" => Some(("MessageAttribute", "Name")),
        "Tags" => Some(("Tag", "Key")),
        _ => None,
    }
}

fn write(out: &mut String, name: &str, value: &Value, action: &str) {
    match value {
        Value::Array(items) => {
            let item = item_name(name, action);
            for value in items {
                write(out, &item, value, action);
            }
        }
        Value::Object(map) => {
            if let Some((item, key)) = map_shape(name) {
                for (k, v) in map {
                    out.push_str(&format!("<{item}><{key}>{}</{key}>", xml_escape(k)));
                    write(out, "Value", v, action);
                    out.push_str(&format!("</{item}>"));
                }
                return;
            }
            out.push_str(&format!("<{name}>"));
            for (k, v) in map {
                write(out, k, v, action);
            }
            out.push_str(&format!("</{name}>"));
        }
        Value::Null => {}
        Value::String(s) => out.push_str(&format!("<{name}>{}</{name}>", xml_escape(s))),
        other => out.push_str(&format!("<{name}>{other}</{name}>")),
    }
}

/// `result`, a JSON object, as `<{action}Response><{action}Result>…`.
pub fn reply(action: &str, result: &Value, request_id: &str) -> Reply {
    let mut body = String::new();
    if let Value::Object(map) = result {
        for (k, v) in map {
            write(&mut body, k, v, action);
        }
    }
    Reply::ok(
        Protocol::Query,
        format!(
            "<?xml version=\"1.0\"?><{action}Response xmlns=\"{NAMESPACE}\"><{action}Result>{body}</{action}Result>\
             <ResponseMetadata><RequestId>{request_id}</RequestId></ResponseMetadata></{action}Response>"
        ),
    )
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn text(reply: &Reply) -> String {
        String::from_utf8(reply.body.to_vec()).unwrap()
    }

    #[test]
    fn writes_lists_flat_and_maps_as_pairs() {
        let list = reply(
            "ListQueues",
            &json!({"QueueUrls": ["http://a/1/x", "http://a/1/y"]}),
            "r",
        );
        assert!(text(&list).contains(
            "<ListQueuesResult><QueueUrl>http://a/1/x</QueueUrl><QueueUrl>http://a/1/y</QueueUrl></ListQueuesResult>"
        ));
        let received = reply(
            "ReceiveMessage",
            &json!({"Messages": [{
                "Body": "hi <there>",
                "Attributes": {"ApproximateReceiveCount": "1"},
                "MessageAttributes": {"kind": {"DataType": "String", "StringValue": "order"}},
            }]}),
            "r",
        );
        let body = text(&received);
        assert!(body.contains("<Message><Body>hi &lt;there&gt;</Body>"));
        assert!(body.contains(
            "<Attribute><Name>ApproximateReceiveCount</Name><Value>1</Value></Attribute>"
        ));
        assert!(body.contains(
            "<MessageAttribute><Name>kind</Name><Value><DataType>String</DataType><StringValue>order</StringValue></Value></MessageAttribute>"
        ));
        let batch = reply(
            "SendMessageBatch",
            &json!({"Successful": [{"Id": "a"}], "Failed": []}),
            "r",
        );
        assert!(
            text(&batch)
                .contains("<SendMessageBatchResultEntry><Id>a</Id></SendMessageBatchResultEntry>")
        );
    }
}
