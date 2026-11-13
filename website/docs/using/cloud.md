---
sidebar_position: 9.7
title: Cloud emulator
description: "Run a local emulator of AWS's APIs from Ferrodock: start it in a moment, copy the settings for the AWS CLI, an SDK or Terraform, and build against it with no account."
---

# Cloud emulator

The **Cloud emulator** page, tagged **AWS** under **CLOUD** in the sidebar,
runs a local emulator of AWS's APIs. Tools that talk to AWS (the AWS CLI, the SDKs, Terraform)
talk to it the same way, so you can build and test against it with no
account and nothing leaving your computer.

The emulator is `ferrocloud`: one small program written in Rust, in an
image of about 3 MB, part of Ferrodock and open source like the rest. It
runs as a container on your engine, only when you start it. Until then
nothing runs.

![The Cloud emulator page with the emulator running](/img/cloud.png)

## Starting it

Until it runs, the page says what the emulator is and how it's used, with
one button: **Start the AWS emulator**. The first time, Ferrodock downloads
the emulator's image and checks it; after that it's up in a moment. The
**AWS** tag in the sidebar turns green while it runs. The page then shows:

- **Where it is**: `http://localhost:4566`, the account `000000000000` and
  the region `us-east-1`. **Copy endpoint** copies the address.
- **Open console**: opens the emulator's web console in your browser, on
  emulator versions that have one.
- **Services**: what the emulator supports.
- **Use it from the AWS CLI or your code**: folded until you open it, what
  to tell each tool, ready to copy.

The square button stops it. Stopped, it uses no memory.

The emulator's container is called `ferrodock-cloud` and is on the
Containers page like any other, with its logs.

## The web console

**Open console** opens the emulator's web console in your browser, at
`http://localhost:4566/_ferrocloud/console/`. The emulator serves it itself,
so nothing else runs, and it works in any browser on this computer. It
shows:

- **Home**: each service, with what it holds.
- **Each service**: what it holds, a tab per kind (IAM's roles, users,
  policies and instance profiles), with a filter. Click a row to see the
  whole record, policy documents laid out as JSON.
- **Requests**: every request the emulator answered, newest first. Click
  one to see what was sent and what came back, secrets masked.

It reads the same data as the AWS APIs and refreshes every few seconds, so
what you create with the AWS CLI, an SDK or Terraform shows up as it
happens.

![The web console showing IAM's roles, one of them opened](/img/cloud-console.png)

## Connecting your tools

Any credentials work: the emulator accepts every key. Under **Use it from
the AWS CLI or your code**, the page has the settings for each tool, with a
**Copy** button:

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

![The Terraform settings on the Cloud emulator page](/img/cloud-terraform.png)

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

## Seeing what your code sent

The **Requests** tab lists every request the emulator answers, as it
answers it: the time, the status, the service, the operation, what it was
about (a role's name, say), and how long it took. A failed request shows
its AWS error code in place.

![The request log on the Cloud emulator page](/img/cloud-requests.png)

- **Filter** by service, tick **Errors only**, or type in the search box
  to find an operation, a resource, an error code or the tool that sent
  it.
- **Click a request** to see it in full: what was sent and what came
  back, laid out for reading (JSON and XML indented, a form one field per
  line), with both sets of headers. For a failure, the error's message is
  at the top.

![A failed request with what was sent and answered](/img/cloud-request.png)

A few things to know:

- **Secrets are masked.** Secret keys, session tokens, passwords and
  request signatures read as `***`, in the log and in what you copy from
  it.
- **Bodies are kept up to 16 KB.** A longer one shows its start and its
  full size; one that isn't text shows only its size.
- **The log is the emulator's memory, not a file.** It holds the last
  1,000 requests and starts again when the emulator does. **Empty…**
  clears it too.
- **Nothing is read while you're not looking.** Ferrodock asks for new
  requests only while the tab is open.

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
