# ferrocloud

Ferrodock's local cloud emulator: one small program that speaks the AWS
APIs, so the AWS CLI, the SDKs and Terraform work against it unchanged.
Ferrodock runs it as a container from its **Cloud emulator** page; it also
runs on its own:

```bash
ferrocloud --port 4566 --data ./cloud-data
aws --endpoint-url http://localhost:4566 sts get-caller-identity
```

Any credentials are accepted. Its web console, at
`http://localhost:4566/_ferrocloud/console/`, shows what each service holds
and every request it answered. What it supports is listed at
`/_ferrocloud/services`; the plan is in
[#354](https://github.com/manimovassagh/Ferrodock/issues/354).

## Its own API

Under `/_ferrocloud/`, for Ferrodock and for scripts:

| Path | What it gives |
| --- | --- |
| `GET health` | Whether it's up, its version and its services |
| `GET services` | Each service and the operations it supports |
| `GET inventory` | What each service holds (IAM's roles, users…), for the console |
| `GET console/` | The web console |
| `GET requests?after=N` | The requests answered after number N |
| `GET requests/N` | One request's headers and bodies, secrets masked |
| `GET events` | The same requests as they happen (server-sent events) |
| `POST reset` | Deletes everything created, and the request log |

## Services

| Service | What works |
| --- | --- |
| STS | `GetCallerIdentity`, `GetSessionToken`, `AssumeRole`, `AssumeRoleWithWebIdentity` |
| IAM | Roles, users, managed policies with versions, inline and attached policies, instance profiles, access keys, tags |

IAM keeps what you create and gives it back; it doesn't enforce it. Every
request is allowed, whatever the policies say.
