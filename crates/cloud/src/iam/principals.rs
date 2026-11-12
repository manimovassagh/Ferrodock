//! Roles and users: the things policies are attached to. What they share
//! (tags, inline policies, attached policies) is written once, by kind.

use serde_json::{Value, json};

use super::records::{self, ACCESS_KEY, POLICY, PROFILE, ROLE, USER};
use super::{arn, conflict, policy_document, xml};
use crate::ACCOUNT;
use crate::clock;
use crate::protocol::Fault;
use crate::query::{Params, element, percent_encode};
use crate::store::Store;
use crate::sts::{id, random};

/// The request field that names a record of this kind.
fn name_field(kind: &str) -> &'static str {
    if kind == ROLE { "RoleName" } else { "UserName" }
}

fn tags_of(params: &Params) -> Value {
    Value::Object(
        params
            .tags("Tags")
            .into_iter()
            .map(|(k, v)| (k, Value::String(v)))
            .collect(),
    )
}

fn path_of(params: &Params) -> String {
    params
        .get("Path")
        .filter(|p| !p.is_empty())
        .unwrap_or("/")
        .to_owned()
}

pub fn create_role(store: &Store, params: &Params) -> Result<String, Fault> {
    let name = params.required("RoleName")?;
    let trust = policy_document(params.required("AssumeRolePolicyDocument")?)?;
    let path = path_of(params);
    let doc = json!({
        "name": name,
        "path": path,
        "id": id("AROA", name),
        "arn": arn("role", &path, name),
        "created": clock::iso8601(clock::now()),
        "assume_role_policy": trust,
        "description": params.get("Description").unwrap_or_default(),
        "max_session": params.get("MaxSessionDuration").and_then(|d| d.parse::<u64>().ok()).unwrap_or(3600),
        "boundary": params.get("PermissionsBoundary").unwrap_or_default(),
        "tags": tags_of(params),
        "inline": {},
        "attached": [],
    });
    records::create(store, ROLE, name, &doc)?;
    Ok(format!("<Role>{}</Role>", xml::role(&doc)))
}

pub fn get_role(store: &Store, params: &Params) -> Result<String, Fault> {
    let doc = records::get(store, ROLE, params.required("RoleName")?)?;
    Ok(format!("<Role>{}</Role>", xml::role(&doc)))
}

pub fn list_roles(store: &Store, params: &Params) -> Result<String, Fault> {
    let prefix = params.get("PathPrefix").unwrap_or("/");
    let roles = records::list(store, ROLE)?;
    Ok(xml::list(
        "Roles",
        roles
            .iter()
            .filter(|r| r["path"].as_str().unwrap_or("/").starts_with(prefix))
            .map(xml::role),
    ))
}

pub fn update_role(store: &Store, params: &Params) -> Result<(), Fault> {
    let name = params.required("RoleName")?;
    let mut doc = records::get(store, ROLE, name)?;
    if let Some(description) = params.get("Description") {
        doc["description"] = json!(description);
    }
    if let Some(seconds) = params
        .get("MaxSessionDuration")
        .and_then(|d| d.parse::<u64>().ok())
    {
        doc["max_session"] = json!(seconds);
    }
    records::put(store, ROLE, name, &doc)
}

pub fn update_assume_role_policy(store: &Store, params: &Params) -> Result<(), Fault> {
    let name = params.required("RoleName")?;
    let mut doc = records::get(store, ROLE, name)?;
    doc["assume_role_policy"] = json!(policy_document(params.required("PolicyDocument")?)?);
    records::put(store, ROLE, name, &doc)
}

pub fn create_user(store: &Store, params: &Params) -> Result<String, Fault> {
    let name = params.required("UserName")?;
    let path = path_of(params);
    let doc = json!({
        "name": name,
        "path": path,
        "id": id("AIDA", name),
        "arn": arn("user", &path, name),
        "created": clock::iso8601(clock::now()),
        "tags": tags_of(params),
        "inline": {},
        "attached": [],
    });
    records::create(store, USER, name, &doc)?;
    Ok(format!("<User>{}</User>", xml::user(&doc)))
}

/// A user, or with no name the caller: the account's root.
pub fn get_user(store: &Store, params: &Params) -> Result<String, Fault> {
    let doc = match params.get("UserName").filter(|n| !n.is_empty()) {
        Some(name) => records::get(store, USER, name)?,
        None => json!({
            "name": "root",
            "path": "/",
            "id": ACCOUNT,
            "arn": format!("arn:aws:iam::{ACCOUNT}:root"),
            "created": clock::iso8601(0),
            "tags": {},
        }),
    };
    Ok(format!("<User>{}</User>", xml::user(&doc)))
}

pub fn list_users(store: &Store, params: &Params) -> Result<String, Fault> {
    let prefix = params.get("PathPrefix").unwrap_or("/");
    let users = records::list(store, USER)?;
    Ok(xml::list(
        "Users",
        users
            .iter()
            .filter(|u| u["path"].as_str().unwrap_or("/").starts_with(prefix))
            .map(xml::user),
    ))
}

/// Deletes a role or user, once nothing hangs on it, as IAM insists.
pub fn delete(store: &Store, kind: &str, params: &Params) -> Result<(), Fault> {
    let name = params.required(name_field(kind))?;
    let doc = records::get(store, kind, name)?;
    let attached = doc["attached"].as_array().is_some_and(|a| !a.is_empty());
    let inline = doc["inline"].as_object().is_some_and(|i| !i.is_empty());
    if attached || inline {
        return Err(conflict(
            "Cannot delete entity, must detach all policies first.",
        ));
    }
    if kind == ROLE {
        let in_profile = records::list(store, PROFILE)?.iter().any(|p| {
            p["roles"]
                .as_array()
                .is_some_and(|r| r.iter().any(|n| n == name))
        });
        if in_profile {
            return Err(conflict(
                "Cannot delete entity, must remove roles from instance profile first.",
            ));
        }
    } else {
        let has_keys = records::list(store, ACCESS_KEY)?
            .iter()
            .any(|k| k["user"] == name);
        if has_keys {
            return Err(conflict(
                "Cannot delete entity, must delete access keys first.",
            ));
        }
    }
    records::delete(store, kind, name)
}

pub fn tag(store: &Store, kind: &str, params: &Params) -> Result<(), Fault> {
    let name = params.required(name_field(kind))?;
    let mut doc = records::get(store, kind, name)?;
    for (key, value) in params.tags("Tags") {
        doc["tags"][key] = json!(value);
    }
    records::put(store, kind, name, &doc)
}

pub fn untag(store: &Store, kind: &str, params: &Params) -> Result<(), Fault> {
    let name = params.required(name_field(kind))?;
    let mut doc = records::get(store, kind, name)?;
    if let Some(tags) = doc["tags"].as_object_mut() {
        for key in params.values("TagKeys") {
            tags.remove(&key);
        }
    }
    records::put(store, kind, name, &doc)
}

pub fn list_tags(store: &Store, kind: &str, params: &Params) -> Result<String, Fault> {
    let doc = records::get(store, kind, params.required(name_field(kind))?)?;
    Ok(xml::tags(&doc) + "<IsTruncated>false</IsTruncated>")
}

// Inline policies: documents kept on the role or user itself.

pub fn put_inline(store: &Store, kind: &str, params: &Params) -> Result<(), Fault> {
    let name = params.required(name_field(kind))?;
    let mut doc = records::get(store, kind, name)?;
    let document = policy_document(params.required("PolicyDocument")?)?;
    doc["inline"][params.required("PolicyName")?] = json!(document);
    records::put(store, kind, name, &doc)
}

pub fn get_inline(store: &Store, kind: &str, params: &Params) -> Result<String, Fault> {
    let name = params.required(name_field(kind))?;
    let policy = params.required("PolicyName")?;
    let doc = records::get(store, kind, name)?;
    let document = doc["inline"][policy]
        .as_str()
        .ok_or_else(|| records::no_such("policy", policy))?;
    Ok(element(name_field(kind), name)
        + &element("PolicyName", policy)
        + &element("PolicyDocument", &percent_encode(document)))
}

pub fn list_inline(store: &Store, kind: &str, params: &Params) -> Result<String, Fault> {
    let doc = records::get(store, kind, params.required(name_field(kind))?)?;
    let names = doc["inline"].as_object().into_iter().flat_map(|i| i.keys());
    Ok(xml::list(
        "PolicyNames",
        names.map(|n| crate::protocol::xml_escape(n)),
    ))
}

pub fn delete_inline(store: &Store, kind: &str, params: &Params) -> Result<(), Fault> {
    let name = params.required(name_field(kind))?;
    let policy = params.required("PolicyName")?;
    let mut doc = records::get(store, kind, name)?;
    let removed = doc["inline"].as_object_mut().and_then(|i| i.remove(policy));
    if removed.is_none() {
        return Err(records::no_such("policy", policy));
    }
    records::put(store, kind, name, &doc)
}

// Attached policies: by ARN. One of this account's has to exist; AWS's own
// (arn:aws:iam::aws:policy/…) are taken as given.

pub fn attach(store: &Store, kind: &str, params: &Params) -> Result<(), Fault> {
    let name = params.required(name_field(kind))?;
    let policy = params.required("PolicyArn")?;
    let mut doc = records::get(store, kind, name)?;
    if policy.contains(&format!("::{ACCOUNT}:")) {
        let policy_name = policy.rsplit('/').next().unwrap_or(policy);
        records::get(store, POLICY, policy_name)?;
    }
    let attached = doc["attached"].as_array_mut().expect("made as a list");
    if !attached.iter().any(|a| a == policy) {
        attached.push(json!(policy));
    }
    records::put(store, kind, name, &doc)
}

pub fn detach(store: &Store, kind: &str, params: &Params) -> Result<(), Fault> {
    let name = params.required(name_field(kind))?;
    let policy = params.required("PolicyArn")?;
    let mut doc = records::get(store, kind, name)?;
    let attached = doc["attached"].as_array_mut().expect("made as a list");
    let before = attached.len();
    attached.retain(|a| a != policy);
    if attached.len() == before {
        return Err(Fault::new(
            404,
            "NoSuchEntity",
            format!("Policy {policy} was not found."),
        ));
    }
    records::put(store, kind, name, &doc)
}

pub fn list_attached(store: &Store, kind: &str, params: &Params) -> Result<String, Fault> {
    let doc = records::get(store, kind, params.required(name_field(kind))?)?;
    let attached = doc["attached"].as_array().into_iter().flatten();
    Ok(xml::list(
        "AttachedPolicies",
        attached.filter_map(Value::as_str).map(|policy| {
            element("PolicyName", policy.rsplit('/').next().unwrap_or(policy))
                + &element("PolicyArn", policy)
        }),
    ))
}

// Access keys. Any key is accepted anyway; these exist so that tools that
// create and list them are satisfied.

pub fn create_access_key(store: &Store, params: &Params) -> Result<String, Fault> {
    let user = params.required("UserName")?;
    records::get(store, USER, user)?;
    let key = random("AKIA", 16);
    let doc = json!({ "id": key, "user": user, "created": clock::iso8601(clock::now()) });
    records::create(store, ACCESS_KEY, &key, &doc)?;
    Ok(format!(
        "<AccessKey>{}{}<Status>Active</Status>{}{}</AccessKey>",
        element("UserName", user),
        element("AccessKeyId", &key),
        element("SecretAccessKey", &random("", 40)),
        element("CreateDate", doc["created"].as_str().unwrap_or_default()),
    ))
}

pub fn list_access_keys(store: &Store, params: &Params) -> Result<String, Fault> {
    let user = params.required("UserName")?;
    records::get(store, USER, user)?;
    let keys = records::list(store, ACCESS_KEY)?;
    Ok(xml::list(
        "AccessKeyMetadata",
        keys.iter().filter(|k| k["user"] == user).map(|k| {
            element("UserName", user)
                + &element("AccessKeyId", k["id"].as_str().unwrap_or_default())
                + "<Status>Active</Status>"
                + &element("CreateDate", k["created"].as_str().unwrap_or_default())
        }),
    ))
}

pub fn delete_access_key(store: &Store, params: &Params) -> Result<(), Fault> {
    records::delete(store, ACCESS_KEY, params.required("AccessKeyId")?)
}
