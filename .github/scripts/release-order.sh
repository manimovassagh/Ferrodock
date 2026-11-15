#!/usr/bin/env bash
# Marks each open release PR (release-plz's, "chore: release vX.Y.Z") with
# a "Release order" status: red while a PR pinning a new cloud emulator
# image (the Cloud workflow's "chore(cloud): use cloud-vX.Y.Z") is still
# open, green otherwise. A release made before that pin is merged would
# ship the previous emulator, so the pin goes first.
#
# Needs GH_TOKEN with `statuses: write` and `pull-requests: read`, and
# GITHUB_REPOSITORY. Safe to run any number of times.
set -euo pipefail

prs=$(gh pr list --repo "$GITHUB_REPOSITORY" --state open --limit 100 \
    --json number,title,headRefName,headRefOid,url)

pins=$(jq -c '[.[] | select(.headRefName | startswith("cloud/pin-"))]' <<<"$prs")
releases=$(jq -c '[.[] | select(.headRefName | startswith("release-plz-"))]' <<<"$prs")

if [ "$(jq length <<<"$pins")" -gt 0 ]; then
    state=failure
    first=$(jq -r 'sort_by(.number) | .[0]' <<<"$pins")
    number=$(jq -r .number <<<"$first")
    title=$(jq -r .title <<<"$first")
    description="Merge #$number ($title) first, so this release ships it"
    target=$(jq -r .url <<<"$first")
else
    state=success
    description="No emulator pin is waiting"
    target=""
fi

jq -c '.[]' <<<"$releases" | while read -r release; do
    sha=$(jq -r .headRefOid <<<"$release")
    number=$(jq -r .number <<<"$release")
    gh api --method POST "repos/$GITHUB_REPOSITORY/statuses/$sha" \
        -f state="$state" -f context="Release order" \
        -f description="${description:0:140}" -f target_url="$target" > /dev/null
    echo "#$number: $state ($description)"
done
