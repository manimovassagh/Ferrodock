---
sidebar_position: 4
title: Connecting to Docker
---

# Connecting to Docker

IronStack connects to the platform's default Docker endpoint:

| Platform | Default |
|---|---|
| Windows | `npipe:////./pipe/docker_engine` |
| macOS, Linux | `unix:///var/run/docker.sock` |

To use a different engine, set `DOCKER_HOST` before starting IronStack:

```bash
# Docker Desktop on macOS, if "Allow the default Docker socket" is off
DOCKER_HOST=unix://$HOME/.docker/run/docker.sock ironstack

# Colima
DOCKER_HOST=unix://$HOME/.colima/default/docker.sock ironstack

# Rancher Desktop (macOS)
DOCKER_HOST=unix://$HOME/.rd/docker.sock ironstack
```

To launch the macOS app bundle with a custom host:

```bash
DOCKER_HOST=unix://$HOME/.colima/default/docker.sock \
  /Applications/IronStack.app/Contents/MacOS/ironstack
```

## When Docker isn't running

IronStack shows a "Docker isn't running" screen and keeps checking in the
background. As soon as the engine is up, it connects on its own.
