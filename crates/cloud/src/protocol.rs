//! What the AWS protocols share: how a reply and an error look in each,
//! request IDs, and XML text.

use std::hash::{BuildHasher, RandomState};

use bytes::Bytes;

/// How a service's requests and replies are encoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    /// JSON bodies, the operation in `X-Amz-Target` (DynamoDB, SQS).
    Json10,
    /// The same with a different content type (Secrets Manager, SSM, ECS).
    Json11,
    /// Form-encoded requests with an `Action`, XML replies (STS, IAM, SNS).
    Query,
    /// The operation in the method and path, XML bodies (S3).
    RestXml,
    /// The operation in the method and path, JSON bodies (Lambda, EKS).
    RestJson,
}

impl Protocol {
    fn content_type(self) -> &'static str {
        match self {
            Protocol::Json10 => "application/x-amz-json-1.0",
            Protocol::Json11 => "application/x-amz-json-1.1",
            Protocol::Query | Protocol::RestXml => "text/xml",
            Protocol::RestJson => "application/json",
        }
    }
}

/// What a service answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Bytes,
    /// The operation this answers, for the request log, when the request
    /// itself didn't name it (the REST protocols).
    pub operation: Option<&'static str>,
}

impl Reply {
    pub fn new(status: u16, content_type: &str, body: impl Into<Bytes>) -> Self {
        Self {
            status,
            headers: vec![("content-type".into(), content_type.into())],
            body: body.into(),
            operation: None,
        }
    }

    pub fn operation(mut self, operation: &'static str) -> Self {
        self.operation = Some(operation);
        self
    }

    /// A 200 with a body in the protocol's content type.
    pub fn ok(protocol: Protocol, body: impl Into<Bytes>) -> Self {
        Self::new(200, protocol.content_type(), body)
    }

    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_owned(), value.to_owned()));
        self
    }
}

/// An error a service answers with, e.g. `NoSuchBucket`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fault {
    pub status: u16,
    pub code: String,
    pub message: String,
}

impl Fault {
    pub fn new(status: u16, code: &str, message: impl Into<String>) -> Self {
        Self {
            status,
            code: code.to_owned(),
            message: message.into(),
        }
    }

    /// The caller asked for something that isn't there or isn't valid.
    pub fn bad_request(code: &str, message: impl Into<String>) -> Self {
        Self::new(400, code, message)
    }

    /// Something went wrong in the emulator itself.
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(500, "InternalFailure", message)
    }

    /// The error as the protocol's clients expect to read it.
    pub fn render(&self, protocol: Protocol, request_id: &str) -> Reply {
        let body = match protocol {
            Protocol::Json10 | Protocol::Json11 | Protocol::RestJson => serde_json::json!({
                "__type": self.code,
                "message": self.message,
            })
            .to_string(),
            Protocol::Query => format!(
                "<ErrorResponse><Error><Type>{}</Type><Code>{}</Code><Message>{}</Message></Error>\
                 <RequestId>{request_id}</RequestId></ErrorResponse>",
                if self.status >= 500 {
                    "Receiver"
                } else {
                    "Sender"
                },
                xml_escape(&self.code),
                xml_escape(&self.message),
            ),
            Protocol::RestXml => format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Error><Code>{}</Code><Message>{}</Message>\
                 <RequestId>{request_id}</RequestId></Error>",
                xml_escape(&self.code),
                xml_escape(&self.message),
            ),
        };
        Reply::new(self.status, protocol.content_type(), body)
            .header("x-amzn-errortype", &self.code)
    }
}

/// A fresh request ID, in the shape of AWS's (a UUID).
pub fn request_id() -> String {
    let a = RandomState::new().hash_one(0u8);
    let b = RandomState::new().hash_one(1u8);
    format!(
        "{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
        a >> 32,
        (a >> 16) & 0xffff,
        a & 0xfff,
        0x8000 | (b >> 48) & 0x3fff,
        b & 0xffff_ffff_ffff
    )
}

/// Text made safe inside an XML element or attribute.
pub fn xml_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(reply: &Reply) -> &str {
        std::str::from_utf8(&reply.body).unwrap()
    }

    #[test]
    fn renders_errors_per_protocol() {
        let fault = Fault::bad_request("ResourceNotFoundException", "No table <t>");
        let json = fault.render(Protocol::Json10, "id-1");
        assert_eq!(json.status, 400);
        assert_eq!(
            text(&json),
            r#"{"__type":"ResourceNotFoundException","message":"No table <t>"}"#
        );
        assert!(
            json.headers
                .contains(&("content-type".into(), "application/x-amz-json-1.0".into()))
        );
        assert!(json.headers.contains(&(
            "x-amzn-errortype".into(),
            "ResourceNotFoundException".into()
        )));

        let query = fault.render(Protocol::Query, "id-1");
        assert!(text(&query).starts_with("<ErrorResponse><Error><Type>Sender</Type>"));
        assert!(text(&query).contains("<Message>No table &lt;t&gt;</Message>"));
        assert!(text(&query).contains("<RequestId>id-1</RequestId>"));

        let rest = Fault::new(404, "NoSuchBucket", "gone").render(Protocol::RestXml, "id-2");
        assert_eq!(rest.status, 404);
        assert!(text(&rest).contains("<Error><Code>NoSuchBucket</Code><Message>gone</Message>"));

        let internal = Fault::internal("oops").render(Protocol::Query, "id-3");
        assert!(text(&internal).contains("<Type>Receiver</Type>"));
    }

    #[test]
    fn request_ids_look_like_uuids_and_differ() {
        let id = request_id();
        let parts: Vec<&str> = id.split('-').collect();
        assert_eq!(
            parts.iter().map(|p| p.len()).collect::<Vec<_>>(),
            [8, 4, 4, 4, 12]
        );
        assert!(id.chars().all(|c| c.is_ascii_hexdigit() || c == '-'));
        assert_ne!(id, request_id());
    }

    #[test]
    fn escapes_xml() {
        assert_eq!(xml_escape(r#"a<b>&"c'"#), "a&lt;b&gt;&amp;&quot;c&apos;");
    }
}
