# Security policy

## Supported versions

Security fixes go into the latest release.

## Reporting a vulnerability

Please **don't** open a public issue for security problems. Report them
privately through
[GitHub security advisories](https://github.com/manimovassagh/IronStack/security/advisories/new)
instead.

You'll get an acknowledgement within a few days, and a fix or mitigation
plan once the report is confirmed.

## Scope

IronStack talks to your local Docker Engine with the same access as the
`docker` CLI. Anyone who can use the Docker socket effectively has root on
the machine, so keep that socket restricted to trusted users.
