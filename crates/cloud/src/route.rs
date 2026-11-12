//! Works out which service and operation a request is for, the way AWS
//! does: from the signature's scope, `X-Amz-Target`, or the `Action` field.

use hyper::HeaderMap;

use crate::DEFAULT_REGION;
use crate::protocol::Protocol;

/// What a request says about where it's going.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    /// The service the request was signed for, e.g. `sqs`.
    pub signing_name: Option<String>,
    /// The first part of `X-Amz-Target`, e.g. `DynamoDB_20120810`.
    pub target_prefix: Option<String>,
    /// The operation, when the protocol names it up front.
    pub operation: Option<String>,
    pub protocol: Protocol,
    pub region: String,
    /// The access key the caller used. Any is accepted.
    pub access_key: Option<String>,
}

/// Reads the route from a request's headers, query and (for form posts)
/// body.
pub fn route(headers: &HeaderMap, query: &[(String, String)], body: &[u8]) -> Route {
    let header = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    let scope = header("authorization")
        .and_then(credential_in_header)
        .or_else(|| {
            query
                .iter()
                .find(|(k, _)| k == "X-Amz-Credential")
                .map(|(_, v)| v.as_str())
        })
        .map(parse_scope)
        .unwrap_or_default();
    let content_type = header("content-type").unwrap_or_default();

    let (target_prefix, target_operation) = match header("x-amz-target") {
        Some(target) => match target.rsplit_once('.') {
            Some((prefix, operation)) => (Some(prefix.to_owned()), Some(operation.to_owned())),
            None => (None, Some(target.to_owned())),
        },
        None => (None, None),
    };
    let form = if content_type.starts_with("application/x-www-form-urlencoded") {
        parse_pairs(&String::from_utf8_lossy(body))
    } else {
        Vec::new()
    };
    let action = form
        .iter()
        .chain(query)
        .find(|(k, _)| k == "Action")
        .map(|(_, v)| v.clone());

    let (protocol, operation) = if target_operation.is_some() {
        let protocol = if content_type.contains("json-1.0") {
            Protocol::Json10
        } else {
            Protocol::Json11
        };
        (protocol, target_operation)
    } else if action.is_some() {
        (Protocol::Query, action)
    } else if matches!(scope.service.as_deref(), None | Some("s3")) {
        (Protocol::RestXml, None)
    } else {
        (Protocol::RestJson, None)
    };
    Route {
        signing_name: scope.service,
        target_prefix,
        operation,
        protocol,
        region: scope.region.unwrap_or_else(|| DEFAULT_REGION.to_owned()),
        access_key: scope.access_key,
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Scope {
    access_key: Option<String>,
    region: Option<String>,
    service: Option<String>,
}

/// The `Credential=` value of a SigV4 `Authorization` header.
fn credential_in_header(authorization: &str) -> Option<&str> {
    let rest = authorization.split_once("Credential=")?.1;
    Some(rest.split([',', ' ']).next().unwrap_or(rest))
}

/// `AKIA…/20261112/us-east-1/sqs/aws4_request` into its parts.
fn parse_scope(credential: &str) -> Scope {
    let mut parts = credential.split('/');
    let access_key = parts.next().filter(|k| !k.is_empty()).map(str::to_owned);
    let _date = parts.next();
    let region = parts.next().filter(|r| !r.is_empty()).map(str::to_owned);
    let service = parts.next().filter(|s| !s.is_empty()).map(str::to_owned);
    Scope {
        access_key,
        region,
        service,
    }
}

/// `a=1&b=x%20y` into pairs, percent-decoded.
pub fn parse_pairs(text: &str) -> Vec<(String, String)> {
    text.split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            (percent_decode(key), percent_decode(value))
        })
        .collect()
}

/// Undoes URL encoding (`%2F`, and `+` for a space).
pub fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 3 <= bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
                match hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    Some(byte) => {
                        out.push(byte);
                        i += 3;
                        continue;
                    }
                    None => out.push(b'%'),
                }
            }
            b'+' => out.push(b' '),
            byte => out.push(byte),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.insert(*name, value.parse().unwrap());
        }
        map
    }

    const AUTH: &str = "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20261112/eu-central-1/dynamodb/aws4_request, SignedHeaders=host;x-amz-date, Signature=abc";

    #[test]
    fn routes_json_by_target() {
        let h = headers(&[
            ("authorization", AUTH),
            ("x-amz-target", "DynamoDB_20120810.PutItem"),
            ("content-type", "application/x-amz-json-1.0"),
        ]);
        let r = route(&h, &[], b"{}");
        assert_eq!(r.signing_name.as_deref(), Some("dynamodb"));
        assert_eq!(r.target_prefix.as_deref(), Some("DynamoDB_20120810"));
        assert_eq!(r.operation.as_deref(), Some("PutItem"));
        assert_eq!(r.protocol, Protocol::Json10);
        assert_eq!(r.region, "eu-central-1");
        assert_eq!(r.access_key.as_deref(), Some("AKIDEXAMPLE"));

        let h = headers(&[
            ("x-amz-target", "secretsmanager.GetSecretValue"),
            ("content-type", "application/x-amz-json-1.1"),
        ]);
        let r = route(&h, &[], b"{}");
        assert_eq!(r.protocol, Protocol::Json11);
        assert_eq!(r.signing_name, None);
        assert_eq!(r.region, DEFAULT_REGION);
    }

    #[test]
    fn routes_query_by_action() {
        let auth = AUTH.replace("dynamodb", "sts");
        let h = headers(&[
            ("authorization", &auth),
            (
                "content-type",
                "application/x-www-form-urlencoded; charset=utf-8",
            ),
        ]);
        let r = route(&h, &[], b"Action=GetCallerIdentity&Version=2011-06-15");
        assert_eq!(r.signing_name.as_deref(), Some("sts"));
        assert_eq!(r.operation.as_deref(), Some("GetCallerIdentity"));
        assert_eq!(r.protocol, Protocol::Query);

        // Older tools put the action in the query string.
        let query = parse_pairs("Action=ListQueues&Version=2012-11-05");
        let r = route(&HeaderMap::new(), &query, b"");
        assert_eq!(r.operation.as_deref(), Some("ListQueues"));
        assert_eq!(r.protocol, Protocol::Query);
    }

    #[test]
    fn routes_rest_by_signing_name() {
        let s3 = AUTH.replace("dynamodb", "s3");
        let r = route(&headers(&[("authorization", &s3)]), &[], b"");
        assert_eq!(r.protocol, Protocol::RestXml);
        assert_eq!(r.operation, None);

        let lambda = AUTH.replace("dynamodb", "lambda");
        let r = route(&headers(&[("authorization", &lambda)]), &[], b"");
        assert_eq!(r.protocol, Protocol::RestJson);

        // Unsigned, and nothing else to go by: S3-style.
        assert_eq!(
            route(&HeaderMap::new(), &[], b"").protocol,
            Protocol::RestXml
        );
    }

    #[test]
    fn reads_presigned_urls() {
        let query = parse_pairs(
            "X-Amz-Algorithm=AWS4-HMAC-SHA256&X-Amz-Credential=AKID%2F20261112%2Fus-west-2%2Fs3%2Faws4_request",
        );
        let r = route(&HeaderMap::new(), &query, b"");
        assert_eq!(r.signing_name.as_deref(), Some("s3"));
        assert_eq!(r.region, "us-west-2");
        assert_eq!(r.access_key.as_deref(), Some("AKID"));
    }

    #[test]
    fn decodes_pairs() {
        assert_eq!(
            parse_pairs("a=1&b=x%20y+z&c&d=%zz&e=50%"),
            [
                ("a".to_owned(), "1".to_owned()),
                ("b".to_owned(), "x y z".to_owned()),
                ("c".to_owned(), String::new()),
                ("d".to_owned(), "%zz".to_owned()),
                ("e".to_owned(), "50%".to_owned()),
            ]
        );
    }
}
