//! STS: who the caller is, and temporary credentials.
//!
//! Nearly every tool asks `GetCallerIdentity` first. Any credentials are
//! accepted, so the temporary ones handed out here are made up, and work.

use std::hash::{BuildHasher, RandomState};

use crate::ACCOUNT;
use crate::clock;
use crate::protocol::Fault;
use crate::query::{self, Params, element};
use crate::service::{Call, Handled, Service};
use crate::store::Store;

const NAMESPACE: &str = "https://sts.amazonaws.com/doc/2011-06-15/";

/// How long temporary credentials last when the caller doesn't say.
const SESSION: u64 = 3600;

pub struct Sts;

impl Service for Sts {
    fn name(&self) -> &'static str {
        "sts"
    }

    fn title(&self) -> &'static str {
        "STS"
    }

    fn signing_names(&self) -> &'static [&'static str] {
        &["sts"]
    }

    fn operations(&self) -> &'static [&'static str] {
        &[
            "AssumeRole",
            "AssumeRoleWithWebIdentity",
            "GetCallerIdentity",
            "GetSessionToken",
        ]
    }

    fn handle<'a>(&'a self, call: Call, _store: &'a Store) -> Handled<'a> {
        Box::pin(async move {
            let params = Params::of(&call);
            let action = call.route.operation.as_deref().unwrap_or_default();
            let result = match action {
                "GetCallerIdentity" => format!(
                    "<Arn>arn:aws:iam::{ACCOUNT}:root</Arn><UserId>{ACCOUNT}</UserId><Account>{ACCOUNT}</Account>"
                ),
                "GetSessionToken" => credentials(duration(&params)),
                "AssumeRole" | "AssumeRoleWithWebIdentity" => {
                    let role = params.required("RoleArn")?;
                    let session = params.required("RoleSessionName")?;
                    let role_name = role.rsplit('/').next().unwrap_or(role);
                    let assumed = format!(
                        "<AssumedRoleUser><AssumedRoleId>{}:{}</AssumedRoleId>\
                         <Arn>arn:aws:sts::{ACCOUNT}:assumed-role/{}/{}</Arn></AssumedRoleUser>",
                        id("AROA", role),
                        query::percent_encode(session),
                        crate::protocol::xml_escape(role_name),
                        crate::protocol::xml_escape(session),
                    );
                    let mut result = credentials(duration(&params)) + &assumed;
                    if action == "AssumeRoleWithWebIdentity" {
                        result += &element("SubjectFromWebIdentityToken", "ferrocloud");
                        result += &element("Audience", "ferrocloud");
                        result += &element("Provider", "ferrocloud");
                    }
                    result
                }
                other => {
                    return Err(Fault::bad_request(
                        "InvalidAction",
                        format!("ferrocloud's STS doesn't support {other} yet"),
                    ));
                }
            };
            Ok(query::reply(action, NAMESPACE, &result, &call.request_id))
        })
    }
}

fn duration(params: &Params) -> u64 {
    params
        .get("DurationSeconds")
        .and_then(|d| d.parse().ok())
        .unwrap_or(SESSION)
}

/// Made-up temporary credentials, in the shape of real ones.
fn credentials(seconds: u64) -> String {
    format!(
        "<Credentials><AccessKeyId>{}</AccessKeyId><SecretAccessKey>{}</SecretAccessKey>\
         <SessionToken>{}</SessionToken><Expiration>{}</Expiration></Credentials>",
        random("ASIA", 16),
        random("", 40),
        random("FQoGZXIvYXdz", 120),
        clock::iso8601(clock::now() + seconds),
    )
}

/// `prefix` and `length` random letters and digits.
pub fn random(prefix: &str, length: usize) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut out = String::from(prefix);
    let mut bits = 0u64;
    for i in 0..length {
        if i % 12 == 0 {
            bits = RandomState::new().hash_one(i);
        }
        out.push(ALPHABET[(bits & 31) as usize] as char);
        bits >>= 5;
    }
    out
}

/// An ID that's always the same for the same name, like `AROA…`.
pub fn id(prefix: &str, name: &str) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    // FNV-1a, so it doesn't change between runs.
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let mut out = String::from(prefix);
    for round in 0..2u8 {
        for byte in name.bytes().chain([round]) {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
        }
        let mut bits = hash;
        for _ in 0..8 {
            out.push(ALPHABET[(bits & 31) as usize] as char);
            bits >>= 5;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn makes_ids_and_secrets() {
        assert_eq!(id("AROA", "app"), id("AROA", "app"));
        assert_ne!(id("AROA", "app"), id("AROA", "api"));
        assert_eq!(id("AROA", "app").len(), 20);
        let key = random("ASIA", 16);
        assert_eq!(key.len(), 20);
        assert!(key.starts_with("ASIA") && key.chars().all(|c| c.is_ascii_alphanumeric()));
        assert_ne!(random("", 40), random("", 40));
    }
}
