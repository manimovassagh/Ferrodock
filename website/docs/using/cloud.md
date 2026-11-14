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
so nothing else runs, and it works in any browser on this computer. It's
laid out like a cloud provider's console:

- **The top bar**: **Services**, a menu of every service by category (the
  ones still to come are shown greyed); a search for services and for
  anything by name (a bucket, a queue, a role), also with ⌘K or Ctrl+K;
  and the region and account.
- **Pinned services** under it: S3, SQS and IAM, and the request log.
- **Console Home**:
  - **Emulator health**: the share of requests that worked, the endpoint
    to copy, how long it's been up, its version, and each service.
  - **Your resources**: how many buckets, objects, queues, messages
    waiting, roles and users there are, each a link.
  - **API traffic**: requests and errors over the last 30 minutes.
  - **Build something**: the command to make a bucket, a queue or a role,
    and to connect your tools, copied in a click.
  - **Recent activity**: the latest requests, live, with the tool that
    sent each.
  - **All services**, by category.
  - **Reset data** empties the emulator; **Create** makes a bucket or a
    queue.
- **A service's page** has its own navigation on the left and what it
  holds in a table with a filter. Click a row to see the whole record,
  policy documents laid out as JSON.
- **S3**: **Create bucket**; a bucket's folders and objects, with
  **Upload** (or files dropped on the page), and each object's details,
  **Download** and **Delete**.
- **SQS**: **Create queue** (a name ending in `.fifo` makes a FIFO queue);
  a queue's messages available, in flight and delayed, its settings and
  dead-letter queue, a form to send a message, and the messages in it,
  looked at without receiving them, so nothing is hidden or counted.
  **Purge** and **Delete** are there too.
- **Requests**: every request the emulator answered, newest first. Click
  one to see what was sent and what came back, laid out as JSON or XML,
  with secrets masked.

It reads the same data as the AWS APIs and refreshes every few seconds,
so what you create with the AWS CLI, an SDK or Terraform shows up as it
happens. What the console does goes through the same APIs, so it shows
in the request log like any other tool's. It follows your system's light
or dark setting.

![The web console's Console Home: emulator health, your resources, API traffic, ways to start and recent activity](/img/cloud-console.png)

![A queue in the web console: its counts and settings, the send form, and its messages](/img/cloud-console-sqs.png)

![A bucket's folder in the web console, one object opened](/img/cloud-console-s3.png)

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
aws s3 mb s3://photos
aws s3 cp ./cat.jpg s3://photos/
aws sqs create-queue --queue-name jobs
```

Version 1 of the AWS CLI doesn't use `AWS_ENDPOINT_URL` for SQS: give it
`--endpoint-url http://localhost:4566` there (version 2 and the SDKs
need nothing more).

![The Terraform settings on the Cloud emulator page](/img/cloud-terraform.png)

## What it supports

The **Services** list comes from the running emulator itself. Click a
service to see its operations, or type in the search box to find one.

| Service | What works |
| --- | --- |
| S3 | Buckets, objects of any size (streamed to disk), uploads in parts, copying, versions, tags, metadata, presigned URLs; buckets by path (`localhost:4566/photos`) or by host (`photos.localhost:4566`). Bucket settings (CORS, policy, lifecycle, website…) are kept and given back |
| SQS | Standard and FIFO queues (message groups, deduplication by ID or by content), long polling, batches of up to ten, visibility timeouts and delays, dead-letter queues, message attributes with the MD5s the SDKs check, tags; in both the JSON and the query protocol |
| IAM | Roles, users, managed policies with versions, inline and attached policies, instance profiles, access keys, tags |
| STS | Who the caller is, session tokens, assuming roles. Tools call it behind the scenes |

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
