//! Managed policies and their versions.

use serde_json::{Value, json};

use super::records::{self, POLICY, ROLE, USER};
use super::{arn, conflict, policy_document, xml};
use crate::clock;
use crate::protocol::Fault;
use crate::query::{Params, element, percent_encode};
use crate::store::Store;
use crate::sts::id;

/// The most versions a policy keeps, as on AWS.
const MAX_VERSIONS: usize = 5;

/// What one of AWS's own policies (`arn:aws:iam::aws:policy/…`) is taken
/// to say: nothing is enforced, so the name is all that matters.
const AWS_MANAGED: &str =
    r#"{"Version":"2012-10-17","Statement":[{"Effect":"Allow","Action":"*","Resource":"*"}]}"#;

/// The policy an ARN names: one of this account's, or a stand-in for one
/// of AWS's own.
fn by_arn(store: &Store, policy_arn: &str) -> Result<Value, Fault> {
    let name = policy_arn.rsplit('/').next().unwrap_or(policy_arn);
    if policy_arn.starts_with("arn:aws:iam::aws:policy/") {
        let created = clock::iso8601(0);
        return Ok(json!({
            "name": name,
            "path": "/",
            "id": id("ANPA", policy_arn),
            "arn": policy_arn,
            "created": created,
            "updated": created,
            "description": "",
            "default_version": "v1",
            "next_version": 2,
            "versions": { "v1": { "document": AWS_MANAGED, "created": created } },
            "tags": {},
        }));
    }
    records::find(store, POLICY, name)?
        .filter(|doc| doc["arn"] == policy_arn)
        .ok_or_else(|| {
            Fault::new(
                404,
                "NoSuchEntity",
                format!("Policy {policy_arn} was not found."),
            )
        })
}

/// The roles and users a policy is attached to.
fn attachments(store: &Store, policy_arn: &str) -> Result<(Vec<Value>, Vec<Value>), Fault> {
    let attached_to = |kind| -> Result<Vec<Value>, Fault> {
        Ok(records::list(store, kind)?
            .into_iter()
            .filter(|doc| {
                doc["attached"]
                    .as_array()
                    .is_some_and(|a| a.iter().any(|p| p == policy_arn))
            })
            .collect())
    };
    Ok((attached_to(ROLE)?, attached_to(USER)?))
}

fn count(store: &Store, policy_arn: &str) -> Result<usize, Fault> {
    let (roles, users) = attachments(store, policy_arn)?;
    Ok(roles.len() + users.len())
}

pub fn create(store: &Store, params: &Params) -> Result<String, Fault> {
    let name = params.required("PolicyName")?;
    let document = policy_document(params.required("PolicyDocument")?)?;
    let path = params.get("Path").filter(|p| !p.is_empty()).unwrap_or("/");
    let now = clock::iso8601(clock::now());
    let doc = json!({
        "name": name,
        "path": path,
        "id": id("ANPA", name),
        "arn": arn("policy", path, name),
        "created": now,
        "updated": now,
        "description": params.get("Description").unwrap_or_default(),
        "default_version": "v1",
        "next_version": 2,
        "versions": { "v1": { "document": document, "created": now } },
        "tags": Value::Object(params.tags("Tags").into_iter().map(|(k, v)| (k, json!(v))).collect()),
    });
    records::create(store, POLICY, name, &doc)?;
    Ok(format!("<Policy>{}</Policy>", xml::policy(&doc, 0)))
}

pub fn get(store: &Store, params: &Params) -> Result<String, Fault> {
    let policy_arn = params.required("PolicyArn")?;
    let doc = by_arn(store, policy_arn)?;
    Ok(format!(
        "<Policy>{}</Policy>",
        xml::policy(&doc, count(store, policy_arn)?)
    ))
}

pub fn list(store: &Store, params: &Params) -> Result<String, Fault> {
    // AWS's own policies aren't listed: there are over a thousand, and
    // they're accepted by name without being here.
    if params.get("Scope") == Some("AWS") {
        return Ok(xml::list("Policies", []));
    }
    let only_attached = params.get("OnlyAttached") == Some("true");
    let prefix = params.get("PathPrefix").unwrap_or("/");
    let mut members = Vec::new();
    for doc in records::list(store, POLICY)? {
        let attached = count(store, doc["arn"].as_str().unwrap_or_default())?;
        if (only_attached && attached == 0)
            || !doc["path"].as_str().unwrap_or("/").starts_with(prefix)
        {
            continue;
        }
        members.push(xml::policy(&doc, attached));
    }
    Ok(xml::list("Policies", members))
}

pub fn delete(store: &Store, params: &Params) -> Result<(), Fault> {
    let policy_arn = params.required("PolicyArn")?;
    let doc = by_arn(store, policy_arn)?;
    if count(store, policy_arn)? > 0 {
        return Err(conflict("Cannot delete a policy attached to entities."));
    }
    if doc["versions"].as_object().is_some_and(|v| v.len() > 1) {
        return Err(conflict(
            "This policy has more than one version. Before you delete a policy, you must delete the policy's versions.",
        ));
    }
    records::delete(store, POLICY, doc["name"].as_str().unwrap_or_default())
}

fn version_xml(doc: &Value, version: &str, with_document: bool) -> String {
    let entry = &doc["versions"][version];
    let mut out = String::new();
    if with_document {
        out += &element(
            "Document",
            &percent_encode(entry["document"].as_str().unwrap_or_default()),
        );
    }
    out + &element("VersionId", version)
        + &element(
            "IsDefaultVersion",
            &(doc["default_version"] == version).to_string(),
        )
        + &element("CreateDate", entry["created"].as_str().unwrap_or_default())
}

pub fn get_version(store: &Store, params: &Params) -> Result<String, Fault> {
    let doc = by_arn(store, params.required("PolicyArn")?)?;
    let version = params.required("VersionId")?;
    if doc["versions"][version].is_null() {
        return Err(Fault::new(
            404,
            "NoSuchEntity",
            format!("Policy version {version} was not found."),
        ));
    }
    Ok(format!(
        "<PolicyVersion>{}</PolicyVersion>",
        version_xml(&doc, version, true)
    ))
}

pub fn list_versions(store: &Store, params: &Params) -> Result<String, Fault> {
    let doc = by_arn(store, params.required("PolicyArn")?)?;
    let versions = doc["versions"]
        .as_object()
        .into_iter()
        .flat_map(|v| v.keys());
    Ok(xml::list(
        "Versions",
        versions.map(|v| version_xml(&doc, v, false)),
    ))
}

pub fn create_version(store: &Store, params: &Params) -> Result<String, Fault> {
    let mut doc = by_arn(store, params.required("PolicyArn")?)?;
    let document = policy_document(params.required("PolicyDocument")?)?;
    if doc["versions"]
        .as_object()
        .is_some_and(|v| v.len() >= MAX_VERSIONS)
    {
        return Err(Fault::new(
            409,
            "LimitExceeded",
            format!(
                "A managed policy can have up to {MAX_VERSIONS} versions. Before you create a new version, you must delete an existing version."
            ),
        ));
    }
    let version = format!("v{}", doc["next_version"].as_u64().unwrap_or(2));
    let now = clock::iso8601(clock::now());
    doc["versions"][&version] = json!({ "document": document, "created": now });
    doc["next_version"] = json!(doc["next_version"].as_u64().unwrap_or(2) + 1);
    doc["updated"] = json!(now);
    if params.get("SetAsDefault") == Some("true") {
        doc["default_version"] = json!(version);
    }
    records::put(
        store,
        POLICY,
        doc["name"].as_str().unwrap_or_default(),
        &doc,
    )?;
    Ok(format!(
        "<PolicyVersion>{}</PolicyVersion>",
        version_xml(&doc, &version, false)
    ))
}

pub fn delete_version(store: &Store, params: &Params) -> Result<(), Fault> {
    let mut doc = by_arn(store, params.required("PolicyArn")?)?;
    let version = params.required("VersionId")?;
    if doc["default_version"] == version {
        return Err(conflict("Cannot delete the default version of a policy."));
    }
    let removed = doc["versions"]
        .as_object_mut()
        .and_then(|v| v.remove(version));
    if removed.is_none() {
        return Err(Fault::new(
            404,
            "NoSuchEntity",
            format!("Policy version {version} was not found."),
        ));
    }
    records::put(
        store,
        POLICY,
        doc["name"].as_str().unwrap_or_default(),
        &doc,
    )
}

pub fn list_entities(store: &Store, params: &Params) -> Result<String, Fault> {
    let policy_arn = params.required("PolicyArn")?;
    by_arn(store, policy_arn)?;
    let (roles, users) = attachments(store, policy_arn)?;
    let members = |docs: &[Value], name: &str, id_field: &str| -> String {
        docs.iter()
            .map(|d| {
                format!(
                    "<member>{}{}</member>",
                    element(name, d["name"].as_str().unwrap_or_default()),
                    element(id_field, d["id"].as_str().unwrap_or_default())
                )
            })
            .collect()
    };
    Ok(format!(
        "<PolicyGroups/><PolicyUsers>{}</PolicyUsers><PolicyRoles>{}</PolicyRoles><IsTruncated>false</IsTruncated>",
        members(&users, "UserName", "UserId"),
        members(&roles, "RoleName", "RoleId"),
    ))
}

pub fn tag(store: &Store, params: &Params) -> Result<(), Fault> {
    let mut doc = by_arn(store, params.required("PolicyArn")?)?;
    for (key, value) in params.tags("Tags") {
        doc["tags"][key] = json!(value);
    }
    records::put(
        store,
        POLICY,
        doc["name"].as_str().unwrap_or_default(),
        &doc,
    )
}

pub fn untag(store: &Store, params: &Params) -> Result<(), Fault> {
    let mut doc = by_arn(store, params.required("PolicyArn")?)?;
    if let Some(tags) = doc["tags"].as_object_mut() {
        for key in params.values("TagKeys") {
            tags.remove(&key);
        }
    }
    records::put(
        store,
        POLICY,
        doc["name"].as_str().unwrap_or_default(),
        &doc,
    )
}

pub fn list_tags(store: &Store, params: &Params) -> Result<String, Fault> {
    let doc = by_arn(store, params.required("PolicyArn")?)?;
    Ok(xml::tags(&doc) + "<IsTruncated>false</IsTruncated>")
}
