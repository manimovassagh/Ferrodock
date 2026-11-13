//! Instance profiles: the wrapper EC2 (and Terraform) put a role in.

use serde_json::{Value, json};

use super::records::{self, PROFILE, ROLE};
use super::{arn, conflict, xml};
use crate::clock;
use crate::protocol::Fault;
use crate::query::Params;
use crate::store::Store;
use crate::sts::id;

/// A profile with its roles looked up, as replies give it.
fn render(store: &Store, doc: &Value) -> Result<String, Fault> {
    let mut roles = Vec::new();
    for name in doc["roles"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        if let Some(role) = records::find(store, ROLE, name)? {
            roles.push(role);
        }
    }
    Ok(xml::profile(doc, &roles))
}

pub fn create(store: &Store, params: &Params) -> Result<String, Fault> {
    let name = params.required("InstanceProfileName")?;
    let path = params.get("Path").filter(|p| !p.is_empty()).unwrap_or("/");
    let doc = json!({
        "name": name,
        "path": path,
        "id": id("AIPA", name),
        "arn": arn("instance-profile", path, name),
        "created": clock::iso8601(clock::now()),
        "roles": [],
        "tags": Value::Object(params.tags("Tags").into_iter().map(|(k, v)| (k, json!(v))).collect()),
    });
    records::create(store, PROFILE, name, &doc)?;
    Ok(format!(
        "<InstanceProfile>{}</InstanceProfile>",
        render(store, &doc)?
    ))
}

pub fn get(store: &Store, params: &Params) -> Result<String, Fault> {
    let doc = records::get(store, PROFILE, params.required("InstanceProfileName")?)?;
    Ok(format!(
        "<InstanceProfile>{}</InstanceProfile>",
        render(store, &doc)?
    ))
}

pub fn delete(store: &Store, params: &Params) -> Result<(), Fault> {
    let name = params.required("InstanceProfileName")?;
    let doc = records::get(store, PROFILE, name)?;
    if doc["roles"].as_array().is_some_and(|r| !r.is_empty()) {
        return Err(conflict(
            "Cannot delete entity, must remove roles from instance profile first.",
        ));
    }
    records::delete(store, PROFILE, name)
}

pub fn add_role(store: &Store, params: &Params) -> Result<(), Fault> {
    let name = params.required("InstanceProfileName")?;
    let role = params.required("RoleName")?;
    let mut doc = records::get(store, PROFILE, name)?;
    records::get(store, ROLE, role)?;
    let roles = doc["roles"].as_array_mut().expect("made as a list");
    if !roles.is_empty() {
        return Err(Fault::new(
            409,
            "LimitExceeded",
            "Cannot exceed quota for InstanceSessionsPerInstanceProfile: 1",
        ));
    }
    roles.push(json!(role));
    records::put(store, PROFILE, name, &doc)
}

pub fn remove_role(store: &Store, params: &Params) -> Result<(), Fault> {
    let name = params.required("InstanceProfileName")?;
    let role = params.required("RoleName")?;
    let mut doc = records::get(store, PROFILE, name)?;
    let roles = doc["roles"].as_array_mut().expect("made as a list");
    let before = roles.len();
    roles.retain(|r| r != role);
    if roles.len() == before {
        return Err(records::no_such("role", role));
    }
    records::put(store, PROFILE, name, &doc)
}

/// Every profile, or with `role` only those that hold it.
pub fn list(store: &Store, role: Option<&str>) -> Result<String, Fault> {
    if let Some(role) = role {
        records::get(store, ROLE, role)?;
    }
    let mut members = Vec::new();
    for doc in records::list(store, PROFILE)? {
        let holds = |role: &str| {
            doc["roles"]
                .as_array()
                .is_some_and(|r| r.iter().any(|n| n == role))
        };
        if role.is_none_or(holds) {
            members.push(render(store, &doc)?);
        }
    }
    Ok(xml::list("InstanceProfiles", members))
}
