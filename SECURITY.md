# Security policy

## Supported versions

Security fixes go into the latest release only. Ferrodock updates itself,
so please make sure you're on the
[latest version](https://github.com/manimovassagh/Ferrodock/releases/latest)
before reporting.

| Version | Supported |
|---|---|
| Latest release | Yes |
| Anything older | No |

The engine images (`engine-v*` releases) follow the app: the version the
latest app downloads is the supported one.

## Reporting a vulnerability

Please **don't** open a public issue, discussion or pull request for
security problems. Report them privately through
[GitHub's private vulnerability reporting](https://github.com/manimovassagh/Ferrodock/security/advisories/new).

Include what you can of:

- what's affected (the app, `fdock`, `ferrodock-mcp`, the engine or its
  guest agent, the updater, the release pipeline) and which version;
- how to reproduce it, or a proof of concept;
- what an attacker could do with it.

## What happens next

- **Within 3 days:** an acknowledgement that the report arrived.
- **Within 10 days:** whether it's confirmed, and a plan for a fix or
  mitigation.
- **Once fixed:** a release with the fix, then a published GitHub security
  advisory (with a CVE where it applies) that credits you, unless you'd
  rather stay anonymous.

Please give us a reasonable chance to release a fix before disclosing
anything publicly; 90 days is the usual upper bound, and we'll agree on a
date with you.

## Scope

In scope: code in this repository and what it ships, including the
release binaries, the engine images, the Homebrew cask and the crates on
crates.io.

Out of scope: vulnerabilities in Docker itself, the Linux kernel, or other
upstream projects (report those upstream; tell us too if Ferrodock ships
an affected version), and anything that needs an attacker who already has
access to your Docker socket.

Ferrodock talks to your Docker Engine with the same access as the
`docker` CLI. Anyone who can use the Docker socket effectively has root on
the machine, so keep that socket restricted to trusted users.

## Verifying downloads

Every release has a `SHA256SUMS.txt` next to its binaries. Check what you
downloaded against it:

```bash
sha256sum --check --ignore-missing SHA256SUMS.txt
```
