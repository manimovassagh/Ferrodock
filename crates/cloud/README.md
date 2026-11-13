# ferrocloud

Ferrodock's local cloud emulator: one small program that speaks the AWS
APIs, so the AWS CLI, the SDKs and Terraform work against it unchanged.
Ferrodock runs it as a container from its **Cloud** page; it also runs on
its own:

```bash
ferrocloud --port 4566 --data ./cloud-data
aws --endpoint-url http://localhost:4566 sts get-caller-identity
```

Any credentials are accepted. What it supports is listed at
`/_ferrocloud/services`; the plan is in
[#354](https://github.com/manimovassagh/Ferrodock/issues/354).
