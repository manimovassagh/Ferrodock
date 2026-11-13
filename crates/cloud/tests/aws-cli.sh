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

# No services yet: each protocol's "not supported" error reaches the CLI
# in a form it understands, naming the service.
fails_with "InvalidAction.*the service 'sts'" aws sts get-caller-identity
fails_with "UnknownOperationException.*the service 'dynamodb'" aws dynamodb list-tables
fails_with "NotImplemented.*the service 's3'" aws s3api list-buckets
fails_with "the service 'lambda'" aws lambda list-functions

echo "all good"
