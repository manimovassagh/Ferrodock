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

# S3: buckets and objects, the way people use them.
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/site/img"
echo "hello" > "$work/hello.txt"
echo "<h1>hi</h1>" > "$work/site/index.html"
echo "png" > "$work/site/img/logo.png"
# Bigger than the CLI's 8 MiB threshold, so it goes up in parts.
head -c 20000000 /dev/urandom > "$work/big.bin"
s3="aws s3 --only-show-errors"

gives 'make_bucket: cli-bucket' aws s3 mb s3://cli-bucket
fails_with BucketAlreadyOwnedByYou aws s3api create-bucket --bucket cli-bucket
fails_with InvalidBucketName aws s3api create-bucket --bucket Not_A_Bucket
gives 'cli-bucket' aws s3 ls
aws s3api head-bucket --bucket cli-bucket > /dev/null
fails_with 404 aws s3api head-bucket --bucket no-such-bucket
gives '/cli-bucket-eu' aws s3api create-bucket --bucket cli-bucket-eu \
    --create-bucket-configuration LocationConstraint=eu-west-1 --region eu-west-1
gives '"LocationConstraint": "eu-west-1"' aws s3api get-bucket-location --bucket cli-bucket-eu

$s3 cp "$work/hello.txt" s3://cli-bucket/hello.txt
gives 'hello' aws s3 cp s3://cli-bucket/hello.txt -
gives '6 hello.txt' aws s3 ls s3://cli-bucket/
# The ETag is the MD5 of the content.
gives 'b1946ac92492d2347c6235b4d2611184' aws s3api head-object --bucket cli-bucket --key hello.txt
fails_with NoSuchKey aws s3api get-object --bucket cli-bucket --key nope.txt "$work/nope"
fails_with NoSuchBucket aws s3 ls s3://no-such-bucket/

# A folder, synced up and back down.
$s3 sync "$work/site" s3://cli-bucket/site
gives 'PRE img/' aws s3 ls s3://cli-bucket/site/
gives 'site/img/logo.png' aws s3 ls s3://cli-bucket --recursive
$s3 sync s3://cli-bucket/site "$work/back"
diff -r "$work/site" "$work/back"
gives '"KeyCount": 2' aws s3api list-objects-v2 --bucket cli-bucket --delimiter / --max-keys 2 --no-paginate
gives 'NextContinuationToken' aws s3api list-objects-v2 --bucket cli-bucket --max-keys 1 --no-paginate

# A large file goes up in parts and comes back the same.
$s3 cp "$work/big.bin" s3://cli-bucket/big.bin
gives '"ContentLength": 20000000' aws s3api head-object --bucket cli-bucket --key big.bin
$s3 cp s3://cli-bucket/big.bin "$work/big.out"
cmp "$work/big.bin" "$work/big.out"
aws s3api get-object --bucket cli-bucket --key hello.txt --range bytes=1-3 "$work/part" > /dev/null
[ "$(cat "$work/part")" = "ell" ]

# Metadata, tags and copies.
aws s3api put-object --bucket cli-bucket --key meta.txt --body "$work/hello.txt" \
    --content-type text/plain --metadata owner=me --tagging 'env=dev' > /dev/null
gives '"owner": "me"' aws s3api head-object --bucket cli-bucket --key meta.txt
gives '"Value": "dev"' aws s3api get-object-tagging --bucket cli-bucket --key meta.txt
aws s3api put-object-tagging --bucket cli-bucket --key meta.txt --tagging 'TagSet=[{Key=team,Value=core}]'
gives '"Key": "team"' aws s3api get-object-tagging --bucket cli-bucket --key meta.txt
$s3 cp s3://cli-bucket/meta.txt s3://cli-bucket-eu/copy.txt
gives '"ContentType": "text/plain"' aws s3api head-object --bucket cli-bucket-eu --key copy.txt
$s3 mv s3://cli-bucket-eu/copy.txt s3://cli-bucket-eu/moved.txt
gives 'moved.txt' aws s3 ls s3://cli-bucket-eu/
fails_with 'Not Found' aws s3api head-object --bucket cli-bucket-eu --key copy.txt

# A presigned URL works with no credentials, and so does the bucket's
# name in the host.
url=$(aws s3 presign s3://cli-bucket/hello.txt --expires-in 60)
[ "$(curl -fsS "$url")" = "hello" ]
host=${AWS_ENDPOINT_URL#http://}
[ "$(curl -fsS -H "Host: cli-bucket.localhost:${host##*:}" "$AWS_ENDPOINT_URL/hello.txt")" = "hello" ]

# Versions: old ones are kept, a delete only hides, and both come back.
aws s3api put-bucket-versioning --bucket cli-bucket --versioning-configuration Status=Enabled
gives '"Status": "Enabled"' aws s3api get-bucket-versioning --bucket cli-bucket
echo "second" > "$work/hello.txt"
$s3 cp "$work/hello.txt" s3://cli-bucket/hello.txt
first=$(aws s3api list-object-versions --bucket cli-bucket --prefix hello.txt \
    --query 'Versions[?IsLatest==`false`].VersionId | [0]' --output text)
aws s3api get-object --bucket cli-bucket --key hello.txt --version-id "$first" "$work/v1" > /dev/null
[ "$(cat "$work/v1")" = "hello" ]
gives '"DeleteMarker": true' aws s3api delete-object --bucket cli-bucket --key hello.txt
fails_with 'Not Found' aws s3api head-object --bucket cli-bucket --key hello.txt
gives '"DeleteMarkers"' aws s3api list-object-versions --bucket cli-bucket --prefix hello.txt
marker=$(aws s3api list-object-versions --bucket cli-bucket --prefix hello.txt \
    --query 'DeleteMarkers[0].VersionId' --output text)
aws s3api delete-object --bucket cli-bucket --key hello.txt --version-id "$marker" > /dev/null
gives 'second' aws s3 cp s3://cli-bucket/hello.txt -

# What Terraform sets on a bucket and reads back.
aws s3api put-bucket-tagging --bucket cli-bucket --tagging 'TagSet=[{Key=env,Value=dev}]'
gives '"Value": "dev"' aws s3api get-bucket-tagging --bucket cli-bucket
aws s3api put-bucket-cors --bucket cli-bucket --cors-configuration \
    'CORSRules=[{AllowedMethods=[GET],AllowedOrigins=[*]}]'
gives '"AllowedMethods"' aws s3api get-bucket-cors --bucket cli-bucket
aws s3api delete-bucket-cors --bucket cli-bucket
fails_with NoSuchCORSConfiguration aws s3api get-bucket-cors --bucket cli-bucket
aws s3api put-bucket-policy --bucket cli-bucket --policy "$allow"
gives 's3:GetObject' aws s3api get-bucket-policy --bucket cli-bucket
fails_with NoSuchLifecycleConfiguration aws s3api get-bucket-lifecycle-configuration --bucket cli-bucket
fails_with ServerSideEncryptionConfigurationNotFoundError aws s3api get-bucket-encryption --bucket cli-bucket
gives 'FULL_CONTROL' aws s3api get-bucket-acl --bucket cli-bucket
aws s3api get-bucket-notification-configuration --bucket cli-bucket > /dev/null

# An upload left unfinished can be seen and abandoned.
upload=$(aws s3api create-multipart-upload --bucket cli-bucket --key half --query UploadId --output text)
gives "$upload" aws s3api list-multipart-uploads --bucket cli-bucket
aws s3api abort-multipart-upload --bucket cli-bucket --key half --upload-id "$upload"
fails_with NoSuchUpload aws s3api list-parts --bucket cli-bucket --key half --upload-id "$upload"

# A bucket goes only once it's empty, versions included.
fails_with BucketNotEmpty aws s3 rb s3://cli-bucket
$s3 rm s3://cli-bucket-eu --recursive
gives 'remove_bucket: cli-bucket-eu' aws s3 rb s3://cli-bucket-eu
aws s3api put-bucket-versioning --bucket cli-bucket --versioning-configuration Status=Suspended
aws s3api list-object-versions --bucket cli-bucket --output json \
    --query '{Objects: [Versions, DeleteMarkers][][].{Key: Key, VersionId: VersionId}, Quiet: `true`}' > "$work/all.json"
aws s3api delete-objects --bucket cli-bucket --delete "file://$work/all.json" > /dev/null
gives 'remove_bucket: cli-bucket' aws s3 rb s3://cli-bucket
gives '"Buckets": \[\]' aws s3api list-buckets

# Services that aren't here yet: each protocol's "not supported" error
# reaches the CLI in a form it understands, naming the service.
fails_with "UnknownOperationException.*the service 'dynamodb'" aws dynamodb list-tables
fails_with "the service 'lambda'" aws lambda list-functions

echo "all good"
