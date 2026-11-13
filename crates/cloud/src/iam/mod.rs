//! IAM: roles, users, policies and instance profiles.
//!
//! Nothing is enforced: every request is allowed whatever the policies
//! say. IAM is here so that tools which set roles and policies up before
//! anything else (Terraform, the CDK, Pulumi) find what they created.
//!
//! - `principals.rs`: roles and users, with their tags, inline policies,
//!   attached policies and access keys.
//! - `policies.rs`: managed policies and their versions.
//! - `profiles.rs`: instance profiles.
//! - `inventory.rs`: what it holds, as the web console lists it.
//! - `records.rs`: how all of them are stored.
//! - `xml.rs`: how they're written in replies.

mod inventory;
mod policies;
mod principals;
mod profiles;
mod records;
mod xml;

use records::{ROLE, USER};

use crate::ACCOUNT;
use crate::protocol::Fault;
use crate::query::{self, Params};
use crate::service::{Call, Handled, Listing, Service};
use crate::store::Store;

const NAMESPACE: &str = "https://iam.amazonaws.com/doc/2010-05-08/";

/// `arn:aws:iam::<account>:<kind><path><name>`.
fn arn(kind: &str, path: &str, name: &str) -> String {
    format!("arn:aws:iam::{ACCOUNT}:{kind}{path}{name}")
}

/// IAM's error for deleting something that's still in use.
fn conflict(message: &str) -> Fault {
    Fault::new(409, "DeleteConflict", message)
}

/// A policy document, which has to be JSON; kept as it was sent.
fn policy_document(text: &str) -> Result<String, Fault> {
    match serde_json::from_str::<serde_json::Value>(text) {
        Ok(value) if value.is_object() => Ok(text.to_owned()),
        _ => Err(Fault::bad_request(
            "MalformedPolicyDocument",
            "This policy contains invalid Json",
        )),
    }
}

/// What an operation gives back.
enum Outcome {
    /// XML for the result element.
    Result(String),
    /// Nothing but the request ID.
    Done,
}

impl From<String> for Outcome {
    fn from(result: String) -> Self {
        Self::Result(result)
    }
}

impl From<()> for Outcome {
    fn from((): ()) -> Self {
        Self::Done
    }
}

pub struct Iam;

impl Service for Iam {
    fn name(&self) -> &'static str {
        "iam"
    }

    fn title(&self) -> &'static str {
        "IAM"
    }

    fn signing_names(&self) -> &'static [&'static str] {
        &["iam"]
    }

    fn operations(&self) -> &'static [&'static str] {
        OPERATIONS
    }

    fn init(&self, store: &Store) -> Result<(), String> {
        records::init(store)
    }

    fn inventory(&self, store: &Store) -> Result<Vec<Listing>, String> {
        inventory::inventory(store)
    }

    fn handle<'a>(&'a self, call: Call, store: &'a Store) -> Handled<'a> {
        Box::pin(async move {
            let params = Params::of(&call);
            let action = call.route.operation.as_deref().unwrap_or_default();
            Ok(match run(action, store, &params)? {
                Outcome::Result(result) => {
                    query::reply(action, NAMESPACE, &result, &call.request_id)
                }
                Outcome::Done => query::empty_reply(action, NAMESPACE, &call.request_id),
            })
        })
    }
}

const OPERATIONS: &[&str] = &[
    "AddRoleToInstanceProfile",
    "AttachRolePolicy",
    "AttachUserPolicy",
    "CreateAccessKey",
    "CreateInstanceProfile",
    "CreatePolicy",
    "CreatePolicyVersion",
    "CreateRole",
    "CreateUser",
    "DeleteAccessKey",
    "DeleteInstanceProfile",
    "DeletePolicy",
    "DeletePolicyVersion",
    "DeleteRole",
    "DeleteRolePolicy",
    "DeleteUser",
    "DeleteUserPolicy",
    "DetachRolePolicy",
    "DetachUserPolicy",
    "GetInstanceProfile",
    "GetPolicy",
    "GetPolicyVersion",
    "GetRole",
    "GetRolePolicy",
    "GetUser",
    "GetUserPolicy",
    "ListAccessKeys",
    "ListAttachedRolePolicies",
    "ListAttachedUserPolicies",
    "ListEntitiesForPolicy",
    "ListGroupsForUser",
    "ListInstanceProfiles",
    "ListInstanceProfilesForRole",
    "ListMFADevices",
    "ListPolicies",
    "ListPolicyTags",
    "ListPolicyVersions",
    "ListRolePolicies",
    "ListRoleTags",
    "ListRoles",
    "ListSSHPublicKeys",
    "ListServiceSpecificCredentials",
    "ListSigningCertificates",
    "ListUserPolicies",
    "ListUserTags",
    "ListUsers",
    "PutRolePolicy",
    "PutUserPolicy",
    "RemoveRoleFromInstanceProfile",
    "TagPolicy",
    "TagRole",
    "TagUser",
    "UntagPolicy",
    "UntagRole",
    "UntagUser",
    "UpdateAssumeRolePolicy",
    "UpdateRole",
];

fn run(action: &str, store: &Store, p: &Params) -> Result<Outcome, Fault> {
    Ok(match action {
        "CreateRole" => principals::create_role(store, p)?.into(),
        "GetRole" => principals::get_role(store, p)?.into(),
        "ListRoles" => principals::list_roles(store, p)?.into(),
        "UpdateRole" => {
            principals::update_role(store, p)?;
            // UpdateRole is the one that answers with an empty result element.
            String::new().into()
        }
        "UpdateAssumeRolePolicy" => principals::update_assume_role_policy(store, p)?.into(),
        "DeleteRole" => principals::delete(store, ROLE, p)?.into(),
        "TagRole" => principals::tag(store, ROLE, p)?.into(),
        "UntagRole" => principals::untag(store, ROLE, p)?.into(),
        "ListRoleTags" => principals::list_tags(store, ROLE, p)?.into(),
        "PutRolePolicy" => principals::put_inline(store, ROLE, p)?.into(),
        "GetRolePolicy" => principals::get_inline(store, ROLE, p)?.into(),
        "ListRolePolicies" => principals::list_inline(store, ROLE, p)?.into(),
        "DeleteRolePolicy" => principals::delete_inline(store, ROLE, p)?.into(),
        "AttachRolePolicy" => principals::attach(store, ROLE, p)?.into(),
        "DetachRolePolicy" => principals::detach(store, ROLE, p)?.into(),
        "ListAttachedRolePolicies" => principals::list_attached(store, ROLE, p)?.into(),

        "CreateUser" => principals::create_user(store, p)?.into(),
        "GetUser" => principals::get_user(store, p)?.into(),
        "ListUsers" => principals::list_users(store, p)?.into(),
        "DeleteUser" => principals::delete(store, USER, p)?.into(),
        "TagUser" => principals::tag(store, USER, p)?.into(),
        "UntagUser" => principals::untag(store, USER, p)?.into(),
        "ListUserTags" => principals::list_tags(store, USER, p)?.into(),
        "PutUserPolicy" => principals::put_inline(store, USER, p)?.into(),
        "GetUserPolicy" => principals::get_inline(store, USER, p)?.into(),
        "ListUserPolicies" => principals::list_inline(store, USER, p)?.into(),
        "DeleteUserPolicy" => principals::delete_inline(store, USER, p)?.into(),
        "AttachUserPolicy" => principals::attach(store, USER, p)?.into(),
        "DetachUserPolicy" => principals::detach(store, USER, p)?.into(),
        "ListAttachedUserPolicies" => principals::list_attached(store, USER, p)?.into(),
        "CreateAccessKey" => principals::create_access_key(store, p)?.into(),
        "ListAccessKeys" => principals::list_access_keys(store, p)?.into(),
        "DeleteAccessKey" => principals::delete_access_key(store, p)?.into(),
        // Things a user can have that ferrocloud doesn't keep: Terraform
        // lists them all before it deletes a user.
        "ListGroupsForUser" => xml::list("Groups", []).into(),
        "ListSSHPublicKeys" => xml::list("SSHPublicKeys", []).into(),
        "ListMFADevices" => xml::list("MFADevices", []).into(),
        "ListSigningCertificates" => xml::list("Certificates", []).into(),
        "ListServiceSpecificCredentials" => "<ServiceSpecificCredentials/>".to_owned().into(),

        "CreatePolicy" => policies::create(store, p)?.into(),
        "GetPolicy" => policies::get(store, p)?.into(),
        "ListPolicies" => policies::list(store, p)?.into(),
        "DeletePolicy" => policies::delete(store, p)?.into(),
        "GetPolicyVersion" => policies::get_version(store, p)?.into(),
        "ListPolicyVersions" => policies::list_versions(store, p)?.into(),
        "CreatePolicyVersion" => policies::create_version(store, p)?.into(),
        "DeletePolicyVersion" => policies::delete_version(store, p)?.into(),
        "ListEntitiesForPolicy" => policies::list_entities(store, p)?.into(),
        "TagPolicy" => policies::tag(store, p)?.into(),
        "UntagPolicy" => policies::untag(store, p)?.into(),
        "ListPolicyTags" => policies::list_tags(store, p)?.into(),

        "CreateInstanceProfile" => profiles::create(store, p)?.into(),
        "GetInstanceProfile" => profiles::get(store, p)?.into(),
        "DeleteInstanceProfile" => profiles::delete(store, p)?.into(),
        "AddRoleToInstanceProfile" => profiles::add_role(store, p)?.into(),
        "RemoveRoleFromInstanceProfile" => profiles::remove_role(store, p)?.into(),
        "ListInstanceProfiles" => profiles::list(store, None)?.into(),
        "ListInstanceProfilesForRole" => {
            profiles::list(store, Some(p.required("RoleName")?))?.into()
        }

        other => {
            return Err(Fault::bad_request(
                "InvalidAction",
                format!("ferrocloud's IAM doesn't support {other} yet"),
            ));
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TRUST: &str = "%7B%22Version%22%3A%222012-10-17%22%7D";

    fn store() -> Store {
        let store = Store::open(None).unwrap();
        records::init(&store).unwrap();
        store
    }

    fn call(store: &Store, form: &str) -> Result<String, Fault> {
        let params = Params::parse(form);
        Ok(match run(params.get("Action").unwrap(), store, &params)? {
            Outcome::Result(xml) => xml,
            Outcome::Done => String::new(),
        })
    }

    #[test]
    fn keeps_roles_with_what_hangs_on_them() {
        let store = store();
        let made = call(
            &store,
            &format!("Action=CreateRole&RoleName=app&Path=/team/&AssumeRolePolicyDocument={TRUST}&Tags.member.1.Key=env&Tags.member.1.Value=dev"),
        )
        .unwrap();
        assert!(
            made.contains("<Arn>arn:aws:iam::000000000000:role/team/app</Arn>"),
            "{made}"
        );
        assert!(made.contains(&format!(
            "<AssumeRolePolicyDocument>{TRUST}</AssumeRolePolicyDocument>"
        )));
        assert!(made.contains("<Key>env</Key><Value>dev</Value>"));

        let again = call(
            &store,
            &format!("Action=CreateRole&RoleName=app&AssumeRolePolicyDocument={TRUST}"),
        );
        assert_eq!(again.unwrap_err().code, "EntityAlreadyExists");
        let broken = call(
            &store,
            "Action=CreateRole&RoleName=other&AssumeRolePolicyDocument=nope",
        );
        assert_eq!(broken.unwrap_err().code, "MalformedPolicyDocument");
        assert_eq!(
            call(&store, "Action=GetRole&RoleName=nobody")
                .unwrap_err()
                .code,
            "NoSuchEntity"
        );

        call(
            &store,
            &format!("Action=PutRolePolicy&RoleName=app&PolicyName=inline&PolicyDocument={TRUST}"),
        )
        .unwrap();
        assert!(
            call(&store, "Action=ListRolePolicies&RoleName=app")
                .unwrap()
                .contains("<member>inline</member>")
        );
        assert_eq!(
            call(&store, "Action=DeleteRole&RoleName=app")
                .unwrap_err()
                .code,
            "DeleteConflict"
        );
        call(
            &store,
            "Action=DeleteRolePolicy&RoleName=app&PolicyName=inline",
        )
        .unwrap();

        call(&store, "Action=UntagRole&RoleName=app&TagKeys.member.1=env").unwrap();
        assert!(
            call(&store, "Action=ListRoleTags&RoleName=app")
                .unwrap()
                .starts_with("<Tags></Tags>")
        );
        call(&store, "Action=DeleteRole&RoleName=app").unwrap();
        assert!(
            call(&store, "Action=ListRoles")
                .unwrap()
                .starts_with("<Roles></Roles>")
        );
    }

    #[test]
    fn attaches_policies_and_versions_them() {
        let store = store();
        let arn = "arn:aws:iam::000000000000:policy/read";
        call(
            &store,
            &format!("Action=CreateRole&RoleName=app&AssumeRolePolicyDocument={TRUST}"),
        )
        .unwrap();
        call(
            &store,
            &format!("Action=CreatePolicy&PolicyName=read&PolicyDocument={TRUST}"),
        )
        .unwrap();

        // One of this account's has to exist; AWS's own are taken as given.
        let missing = call(
            &store,
            "Action=AttachRolePolicy&RoleName=app&PolicyArn=arn:aws:iam::000000000000:policy/none",
        );
        assert_eq!(missing.unwrap_err().code, "NoSuchEntity");
        call(
            &store,
            &format!("Action=AttachRolePolicy&RoleName=app&PolicyArn={arn}"),
        )
        .unwrap();
        call(
            &store,
            "Action=AttachRolePolicy&RoleName=app&PolicyArn=arn:aws:iam::aws:policy/ReadOnlyAccess",
        )
        .unwrap();
        let attached = call(&store, "Action=ListAttachedRolePolicies&RoleName=app").unwrap();
        assert!(
            attached.contains("<PolicyName>read</PolicyName>")
                && attached.contains("ReadOnlyAccess")
        );
        assert!(
            call(&store, &format!("Action=GetPolicy&PolicyArn={arn}"))
                .unwrap()
                .contains("<AttachmentCount>1</AttachmentCount>")
        );
        assert!(
            call(
                &store,
                "Action=GetPolicy&PolicyArn=arn:aws:iam::aws:policy/ReadOnlyAccess"
            )
            .unwrap()
            .contains("ReadOnlyAccess")
        );
        assert_eq!(
            call(&store, &format!("Action=DeletePolicy&PolicyArn={arn}"))
                .unwrap_err()
                .code,
            "DeleteConflict"
        );

        let v2 = call(&store, &format!("Action=CreatePolicyVersion&PolicyArn={arn}&PolicyDocument={TRUST}&SetAsDefault=true")).unwrap();
        assert!(
            v2.contains("<VersionId>v2</VersionId><IsDefaultVersion>true</IsDefaultVersion>"),
            "{v2}"
        );
        let default = call(
            &store,
            &format!("Action=DeletePolicyVersion&PolicyArn={arn}&VersionId=v2"),
        );
        assert_eq!(default.unwrap_err().code, "DeleteConflict");
        for _ in 0..3 {
            call(
                &store,
                &format!("Action=CreatePolicyVersion&PolicyArn={arn}&PolicyDocument={TRUST}"),
            )
            .unwrap();
        }
        let sixth = call(
            &store,
            &format!("Action=CreatePolicyVersion&PolicyArn={arn}&PolicyDocument={TRUST}"),
        );
        assert_eq!(sixth.unwrap_err().code, "LimitExceeded");

        call(
            &store,
            &format!("Action=DetachRolePolicy&RoleName=app&PolicyArn={arn}"),
        )
        .unwrap();
        let twice = call(
            &store,
            &format!("Action=DetachRolePolicy&RoleName=app&PolicyArn={arn}"),
        );
        assert_eq!(twice.unwrap_err().code, "NoSuchEntity");
    }

    #[test]
    fn wraps_a_role_in_an_instance_profile() {
        let store = store();
        call(
            &store,
            &format!("Action=CreateRole&RoleName=app&AssumeRolePolicyDocument={TRUST}"),
        )
        .unwrap();
        call(
            &store,
            "Action=CreateInstanceProfile&InstanceProfileName=box",
        )
        .unwrap();
        call(
            &store,
            "Action=AddRoleToInstanceProfile&InstanceProfileName=box&RoleName=app",
        )
        .unwrap();
        let profile = call(&store, "Action=GetInstanceProfile&InstanceProfileName=box").unwrap();
        assert!(profile.contains("<RoleName>app</RoleName>"), "{profile}");
        assert!(
            call(&store, "Action=ListInstanceProfilesForRole&RoleName=app")
                .unwrap()
                .contains("box")
        );
        assert_eq!(
            call(&store, "Action=DeleteRole&RoleName=app")
                .unwrap_err()
                .code,
            "DeleteConflict"
        );
        let full = call(
            &store,
            "Action=DeleteInstanceProfile&InstanceProfileName=box",
        );
        assert_eq!(full.unwrap_err().code, "DeleteConflict");
        call(
            &store,
            "Action=RemoveRoleFromInstanceProfile&InstanceProfileName=box&RoleName=app",
        )
        .unwrap();
        call(
            &store,
            "Action=DeleteInstanceProfile&InstanceProfileName=box",
        )
        .unwrap();
    }

    #[test]
    fn gives_users_access_keys() {
        let store = store();
        assert!(
            call(&store, "Action=GetUser")
                .unwrap()
                .contains(":root</Arn>")
        );
        call(&store, "Action=CreateUser&UserName=dev").unwrap();
        let key = call(&store, "Action=CreateAccessKey&UserName=dev").unwrap();
        let id = key
            .split("<AccessKeyId>")
            .nth(1)
            .unwrap()
            .split('<')
            .next()
            .unwrap();
        assert!(id.starts_with("AKIA") && key.contains("<SecretAccessKey>"));
        assert!(
            call(&store, "Action=ListAccessKeys&UserName=dev")
                .unwrap()
                .contains(id)
        );
        assert_eq!(
            call(&store, "Action=DeleteUser&UserName=dev")
                .unwrap_err()
                .code,
            "DeleteConflict"
        );
        call(&store, &format!("Action=DeleteAccessKey&AccessKeyId={id}")).unwrap();
        call(&store, "Action=DeleteUser&UserName=dev").unwrap();
        assert_eq!(
            call(&store, "Action=Nope").unwrap_err().code,
            "InvalidAction"
        );
    }

    #[test]
    fn lists_every_operation_it_handles() {
        let store = store();
        for operation in OPERATIONS {
            let outcome = call(&store, &format!("Action={operation}"));
            assert!(
                outcome.is_ok() || outcome.unwrap_err().code != "InvalidAction",
                "{operation}"
            );
        }
    }
}
