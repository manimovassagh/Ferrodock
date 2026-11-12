#!/usr/bin/env bash
# What the real AWS CLI gets from a running ferrocloud. Each service adds
# its everyday commands here as it arrives.
#
# Needs AWS_ENDPOINT_URL (or the default, http://localhost:4566), and any
# credentials and region in the environment.
set -euo pipefail

export AWS_ENDPOINT_URL=${AWS_ENDPOINT_URL:-http://localhost:4566}
export AWS_ACCESS_KEY_ID=${AWS_ACCESS_KEY_ID:-test}
export AWS_SECRET_ACCESS_KEY=${AWS_SECRET_ACCESS_KEY:-test}
export AWS_DEFAULT_REGION=${AWS_DEFAULT_REGION:-us-east-1}
export AWS_PAGER=""

# Runs a command that should fail, and checks the error the CLI prints.
fails_with() {
    local expected=$1
    shift
    local output
    if output=$("$@" 2>&1); then
        echo "expected '$*' to fail with $expected, but it worked:" >&2
        echo "$output" >&2
        exit 1
    fi
    if ! grep -q "$expected" <<<"$output"; then
        echo "expected '$*' to fail with $expected, got:" >&2
        echo "$output" >&2
        exit 1
    fi
    echo "ok: $* -> $expected"
}

# Runs a command and checks its output holds the text.
gives() {
    local expected=$1
    shift
    local output
    output=$("$@")
    if ! grep -q -- "$expected" <<<"$output"; then
        echo "expected '$*' to give $expected, got:" >&2
        echo "$output" >&2
        exit 1
    fi
    echo "ok: $* -> $expected"
}

trust='{"Version":"2012-10-17","Statement":[{"Effect":"Allow","Principal":{"Service":"lambda.amazonaws.com"},"Action":"sts:AssumeRole"}]}'
allow='{"Version":"2012-10-17","Statement":[{"Effect":"Allow","Action":"s3:GetObject","Resource":"*"}]}'

# STS: who am I, and temporary credentials.
gives '"Account": "000000000000"' aws sts get-caller-identity
gives '"AccessKeyId": "ASIA' aws sts get-session-token
gives 'assumed-role/cli-role/me' aws sts assume-role \
    --role-arn arn:aws:iam::000000000000:role/cli-role --role-session-name me

# IAM: a role with tags, an inline policy and attached policies.
gives 'arn:aws:iam::000000000000:role/cli-role' aws iam create-role --role-name cli-role \
    --assume-role-policy-document "$trust" --description "made by the CLI" --tags Key=env,Value=dev
fails_with EntityAlreadyExists aws iam create-role --role-name cli-role --assume-role-policy-document "$trust"
fails_with MalformedPolicyDocument aws iam create-role --role-name broken --assume-role-policy-document 'not json'
gives 'lambda.amazonaws.com' aws iam get-role --role-name cli-role
gives 'made by the CLI' aws iam get-role --role-name cli-role
gives 'cli-role' aws iam list-roles
aws iam tag-role --role-name cli-role --tags Key=team,Value=core
gives '"Key": "team"' aws iam list-role-tags --role-name cli-role
aws iam untag-role --role-name cli-role --tag-keys env
fails_with NoSuchEntity aws iam get-role --role-name nobody

aws iam put-role-policy --role-name cli-role --policy-name inline --policy-document "$allow"
gives 's3:GetObject' aws iam get-role-policy --role-name cli-role --policy-name inline
gives '"inline"' aws iam list-role-policies --role-name cli-role

gives 'arn:aws:iam::000000000000:policy/cli-policy' aws iam create-policy --policy-name cli-policy --policy-document "$allow"
policy=arn:aws:iam::000000000000:policy/cli-policy
gives '"VersionId": "v2"' aws iam create-policy-version --policy-arn "$policy" --policy-document "$allow" --set-as-default
gives '"DefaultVersionId": "v2"' aws iam get-policy --policy-arn "$policy"
gives 's3:GetObject' aws iam get-policy-version --policy-arn "$policy" --version-id v2
aws iam attach-role-policy --role-name cli-role --policy-arn "$policy"
aws iam attach-role-policy --role-name cli-role --policy-arn arn:aws:iam::aws:policy/ReadOnlyAccess
gives 'ReadOnlyAccess' aws iam list-attached-role-policies --role-name cli-role
gives '"AttachmentCount": 1' aws iam get-policy --policy-arn "$policy"
gives '"RoleName": "cli-role"' aws iam list-entities-for-policy --policy-arn "$policy"
fails_with DeleteConflict aws iam delete-policy --policy-arn "$policy"
fails_with DeleteConflict aws iam delete-role --role-name cli-role

# An instance profile around the role.
gives 'instance-profile/cli-profile' aws iam create-instance-profile --instance-profile-name cli-profile
aws iam add-role-to-instance-profile --instance-profile-name cli-profile --role-name cli-role
gives '"RoleName": "cli-role"' aws iam get-instance-profile --instance-profile-name cli-profile
gives 'cli-profile' aws iam list-instance-profiles-for-role --role-name cli-role

# A user with an access key.
gives 'arn:aws:iam::000000000000:user/cli-user' aws iam create-user --user-name cli-user
gives '"AccessKeyId": "AKIA' aws iam create-access-key --user-name cli-user
key=$(aws iam list-access-keys --user-name cli-user --query 'AccessKeyMetadata[0].AccessKeyId' --output text)
fails_with DeleteConflict aws iam delete-user --user-name cli-user
aws iam delete-access-key --user-name cli-user --access-key-id "$key"
aws iam delete-user --user-name cli-user

# And everything taken down again, in the order IAM insists on.
aws iam remove-role-from-instance-profile --instance-profile-name cli-profile --role-name cli-role
aws iam delete-instance-profile --instance-profile-name cli-profile
aws iam detach-role-policy --role-name cli-role --policy-arn "$policy"
aws iam detach-role-policy --role-name cli-role --policy-arn arn:aws:iam::aws:policy/ReadOnlyAccess
aws iam delete-role-policy --role-name cli-role --policy-name inline
aws iam delete-role --role-name cli-role
aws iam delete-policy-version --policy-arn "$policy" --version-id v1
aws iam delete-policy --policy-arn "$policy"
gives '"Roles": \[\]' aws iam list-roles
gives '"Policies": \[\]' aws iam list-policies --scope Local

# Services that aren't here yet: each protocol's "not supported" error
# reaches the CLI in a form it understands, naming the service.
fails_with "UnknownOperationException.*the service 'dynamodb'" aws dynamodb list-tables
fails_with "NotImplemented.*the service 's3'" aws s3api list-buckets
fails_with "the service 'lambda'" aws lambda list-functions

echo "all good"
