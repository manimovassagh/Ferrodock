---
sidebar_position: 9.7
title: Cloud
description: "Run a local emulator of AWS's APIs from Ferrodock: start it in a moment, copy the settings for the AWS CLI, an SDK or Terraform, and build against it with no account."
---

# Cloud

The **Cloud** page, under **CLOUD** in the sidebar, runs a local emulator
of AWS's APIs. Tools that talk to AWS (the AWS CLI, the SDKs, Terraform)
talk to it the same way, so you can build and test against it with no
account and nothing leaving your computer.

The emulator is `ferrocloud`: one small program written in Rust, in an
image of about 3 MB, part of Ferrodock and open source like the rest. It
runs as a container on your engine, only when you start it. Until then
nothing runs.

![The Cloud page with the emulator running](/img/cloud.png)

## Starting it

Click **Start**. The first time, Ferrodock downloads the emulator's image
and checks it; after that it's up in a moment. The page then shows:

- **Where it is**: `http://localhost:4566`, the account `000000000000` and
  the region `us-east-1`. **Copy endpoint** copies the address.
- **Connect**: what to tell each tool, ready to copy.
- **Services**: what the emulator supports.

The square button stops it. Stopped, it uses no memory.

The emulator's container is called `ferrodock-cloud` and is on the
Containers page like any other, with its logs.

## Connecting your tools

Any credentials work: the emulator accepts every key. The page has the
settings for each tool, with a **Copy** button:

- **Shell** and **PowerShell**: four environment variables. The AWS CLI and
  every AWS SDK read them, so this is all most tools need:

  ```bash
  export AWS_ENDPOINT_URL=http://localhost:4566
  export AWS_ACCESS_KEY_ID=test
  export AWS_SECRET_ACCESS_KEY=test
  export AWS_DEFAULT_REGION=us-east-1
  ```

- **AWS CLI**: a profile for `~/.aws/config`, used with
  `--profile ferrodock`, when you'd rather not change your environment.
- **Terraform**: the `provider "aws"` block. List each service you use
  under `endpoints`.
- **Python** and **JavaScript**: a client pointed at the emulator.

Then try it:

```bash
aws sts get-caller-identity
```

![The Terraform settings on the Cloud page](/img/cloud-terraform.png)

## What it supports

The **Services** list comes from the running emulator itself. Click a
service to see its operations, or type in the search box to find one.

| Service | What works |
| --- | --- |
| STS | Who the caller is, session tokens, assuming roles |
| IAM | Roles, users, managed policies with versions, inline and attached policies, instance profiles, access keys, tags |

More services arrive with each version: the plan is in
[the project's issues](https://github.com/manimovassagh/Ferrodock/issues/354).

Two things to know:

- **Nothing is enforced.** IAM keeps the roles and policies you create and
  gives them back, but every request is allowed whatever they say.
- **An operation that isn't there says so.** The emulator answers with an
  error naming the service and operation, in the form your tool expects.

## Your data

What you create is kept in the volume `ferrodock-cloud-data`, so it's
there when the emulator starts again.

- **Empty…** deletes everything created in the emulator, which keeps
  running. Ferrodock asks first.
- When it's stopped, **Remove its data…** deletes its container and the
  volume.
- To start empty every time, turn off **Keep what's created in the
  emulator between starts** in [Settings](./settings.md#cloud).

## Another port

The emulator listens on port 4566, which AWS emulators conventionally use.
If something else has that port, starting says so: change **Port of the
cloud emulator** in [Settings](./settings.md#cloud) and start again. The
page's settings follow the port.

## From a terminal

`fdock cloud` does the same without the app:

```bash
fdock cloud start
eval "$(fdock cloud env)"
aws sts get-caller-identity
```

See [CLI and MCP](../cli-and-mcp.md#cloud).
