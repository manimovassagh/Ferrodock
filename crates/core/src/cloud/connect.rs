//! What each tool needs to be told to use the emulator instead of AWS.
//! Any credentials work; `test` is what people expect to see.

use super::{CLOUD_REGION, endpoint};

/// The key and secret the snippets use. The emulator accepts any.
const KEY: &str = "test";

/// A piece of configuration to copy.
pub struct Snippet {
    /// What it's for, e.g. "Terraform".
    pub title: &'static str,
    /// Where it goes, e.g. "Add to ~/.aws/config".
    pub hint: &'static str,
    text: fn(&str) -> String,
}

impl Snippet {
    /// The text to copy, for the emulator on `port`.
    pub fn text(&self, port: u16) -> String {
        (self.text)(&endpoint(port))
    }
}

/// `NAME=value` for each variable tools read, for the emulator on `port`.
pub fn env_lines(port: u16) -> Vec<(&'static str, String)> {
    vec![
        ("AWS_ENDPOINT_URL", endpoint(port)),
        ("AWS_ACCESS_KEY_ID", KEY.to_owned()),
        ("AWS_SECRET_ACCESS_KEY", KEY.to_owned()),
        ("AWS_DEFAULT_REGION", CLOUD_REGION.to_owned()),
    ]
}

fn shell(endpoint: &str) -> String {
    format!(
        "export AWS_ENDPOINT_URL={endpoint}\nexport AWS_ACCESS_KEY_ID={KEY}\n\
         export AWS_SECRET_ACCESS_KEY={KEY}\nexport AWS_DEFAULT_REGION={CLOUD_REGION}"
    )
}

fn powershell(endpoint: &str) -> String {
    format!(
        "$env:AWS_ENDPOINT_URL = \"{endpoint}\"\n$env:AWS_ACCESS_KEY_ID = \"{KEY}\"\n\
         $env:AWS_SECRET_ACCESS_KEY = \"{KEY}\"\n$env:AWS_DEFAULT_REGION = \"{CLOUD_REGION}\""
    )
}

fn profile(endpoint: &str) -> String {
    format!(
        "[profile ferrodock]\nregion = {CLOUD_REGION}\nendpoint_url = {endpoint}\n\
         aws_access_key_id = {KEY}\naws_secret_access_key = {KEY}"
    )
}

fn terraform(endpoint: &str) -> String {
    format!(
        "provider \"aws\" {{\n  region                      = \"{CLOUD_REGION}\"\n  access_key                  = \"{KEY}\"\n  secret_key                  = \"{KEY}\"\n  skip_credentials_validation = true\n  skip_metadata_api_check     = true\n  skip_requesting_account_id  = true\n  s3_use_path_style           = true\n\n  endpoints {{\n    iam = \"{endpoint}\"\n    sts = \"{endpoint}\"\n  }}\n}}"
    )
}

fn python(endpoint: &str) -> String {
    format!(
        "import boto3\n\nsession = boto3.Session(\n    aws_access_key_id=\"{KEY}\",\n    aws_secret_access_key=\"{KEY}\",\n    region_name=\"{CLOUD_REGION}\",\n)\nsts = session.client(\"sts\", endpoint_url=\"{endpoint}\")\nprint(sts.get_caller_identity()[\"Account\"])"
    )
}

fn javascript(endpoint: &str) -> String {
    format!(
        "import {{ STSClient, GetCallerIdentityCommand }} from \"@aws-sdk/client-sts\";\n\nconst sts = new STSClient({{\n  endpoint: \"{endpoint}\",\n  region: \"{CLOUD_REGION}\",\n  credentials: {{ accessKeyId: \"{KEY}\", secretAccessKey: \"{KEY}\" }},\n}});\nconsole.log((await sts.send(new GetCallerIdentityCommand({{}}))).Account);"
    )
}

/// Every snippet, in the order they're shown.
pub const SNIPPETS: &[Snippet] = &[
    Snippet {
        title: "Shell",
        hint: "Paste into a terminal: the AWS CLI and every SDK read these variables",
        text: shell,
    },
    Snippet {
        title: "PowerShell",
        hint: "The same variables, for PowerShell",
        text: powershell,
    },
    Snippet {
        title: "AWS CLI",
        hint: "Add to ~/.aws/config, then use --profile ferrodock",
        text: profile,
    },
    Snippet {
        title: "Terraform",
        hint: "The provider block; list each service you use under endpoints",
        text: terraform,
    },
    Snippet {
        title: "Python",
        hint: "A boto3 client pointed at the emulator",
        text: python,
    },
    Snippet {
        title: "JavaScript",
        hint: "AWS SDK for JavaScript v3",
        text: javascript,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_snippet_names_the_endpoint() {
        let endpoint = endpoint(4570);
        assert!(endpoint.ends_with(":4570"));
        for snippet in SNIPPETS {
            let text = snippet.text(4570);
            assert!(text.contains(&endpoint), "{}", snippet.title);
            assert!(
                !text.contains("{endpoint}") && !text.contains("{{"),
                "{}: {text}",
                snippet.title
            );
            assert!(!snippet.title.ends_with('.') && !snippet.hint.ends_with('.'));
        }
        assert_eq!(env_lines(4570)[0], ("AWS_ENDPOINT_URL", endpoint));
    }
}
