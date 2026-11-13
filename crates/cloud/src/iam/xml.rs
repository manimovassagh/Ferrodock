//! IAM's records as the XML its replies carry.

use serde_json::Value;

use crate::query::{element, percent_encode};

fn text<'a>(doc: &'a Value, field: &str) -> &'a str {
    doc[field].as_str().unwrap_or_default()
}

/// `<Tags><member><Key/><Value/></member>…</Tags>`.
pub fn tags(doc: &Value) -> String {
    let members: String = doc["tags"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(key, value)| {
            format!(
                "<member>{}{}</member>",
                element("Key", key),
                element("Value", value.as_str().unwrap_or_default())
            )
        })
        .collect();
    format!("<Tags>{members}</Tags>")
}

/// The fields a role has, without the element around them.
pub fn role(doc: &Value) -> String {
    let mut out = element("Path", text(doc, "path"))
        + &element("RoleName", text(doc, "name"))
        + &element("RoleId", text(doc, "id"))
        + &element("Arn", text(doc, "arn"))
        + &element("CreateDate", text(doc, "created"))
        + &element(
            "AssumeRolePolicyDocument",
            &percent_encode(text(doc, "assume_role_policy")),
        )
        + &element(
            "MaxSessionDuration",
            &doc["max_session"].as_u64().unwrap_or(3600).to_string(),
        );
    if let Some(description) = doc["description"].as_str().filter(|d| !d.is_empty()) {
        out += &element("Description", description);
    }
    if let Some(boundary) = doc["boundary"].as_str().filter(|b| !b.is_empty()) {
        out += &format!(
            "<PermissionsBoundary><PermissionsBoundaryType>Policy</PermissionsBoundaryType>{}</PermissionsBoundary>",
            element("PermissionsBoundaryArn", boundary)
        );
    }
    out + &tags(doc) + "<RoleLastUsed/>"
}

pub fn user(doc: &Value) -> String {
    element("Path", text(doc, "path"))
        + &element("UserName", text(doc, "name"))
        + &element("UserId", text(doc, "id"))
        + &element("Arn", text(doc, "arn"))
        + &element("CreateDate", text(doc, "created"))
        + &tags(doc)
}

pub fn policy(doc: &Value, attachments: usize) -> String {
    let mut out = element("PolicyName", text(doc, "name"))
        + &element("PolicyId", text(doc, "id"))
        + &element("Arn", text(doc, "arn"))
        + &element("Path", text(doc, "path"))
        + &element("DefaultVersionId", text(doc, "default_version"))
        + &element("AttachmentCount", &attachments.to_string())
        + "<PermissionsBoundaryUsageCount>0</PermissionsBoundaryUsageCount><IsAttachable>true</IsAttachable>"
        + &element("CreateDate", text(doc, "created"))
        + &element("UpdateDate", text(doc, "updated"));
    if let Some(description) = doc["description"].as_str().filter(|d| !d.is_empty()) {
        out += &element("Description", description);
    }
    out + &tags(doc)
}

/// An instance profile with its roles, given in full.
pub fn profile(doc: &Value, roles: &[Value]) -> String {
    let members: String = roles
        .iter()
        .map(|r| format!("<member>{}</member>", role(r)))
        .collect();
    element("Path", text(doc, "path"))
        + &element("InstanceProfileName", text(doc, "name"))
        + &element("InstanceProfileId", text(doc, "id"))
        + &element("Arn", text(doc, "arn"))
        + &element("CreateDate", text(doc, "created"))
        + &format!("<Roles>{members}</Roles>")
        + &tags(doc)
}

/// `<name><member>…</member>…</name><IsTruncated>false</IsTruncated>`:
/// a list that's always given whole.
pub fn list(name: &str, members: impl IntoIterator<Item = String>) -> String {
    let members: String = members
        .into_iter()
        .map(|m| format!("<member>{m}</member>"))
        .collect();
    format!("<{name}>{members}</{name}><IsTruncated>false</IsTruncated>")
}
