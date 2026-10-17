---
sidebar_position: 4
title: Connecting to Docker
---

# Connecting to Docker

Ferrodock connects to the platform's default Docker endpoint:

| Platform | Default |
|---|---|
| Windows | `npipe:////./pipe/docker_engine` |
| macOS, Linux | `unix:///var/run/docker.sock` |

To use a different engine, set `DOCKER_HOST` before starting Ferrodock:

```bash
# Docker Desktop on macOS, if "Allow the default Docker socket" is off
DOCKER_HOST=unix://$HOME/.docker/run/docker.sock ferrodock

# Colima
DOCKER_HOST=unix://$HOME/.colima/default/docker.sock ferrodock

# Rancher Desktop (macOS)
DOCKER_HOST=unix://$HOME/.rd/docker.sock ferrodock
```

To launch the macOS app bundle with a custom host:

```bash
DOCKER_HOST=unix://$HOME/.colima/default/docker.sock \
  /Applications/Ferrodock.app/Contents/MacOS/ferrodock
```

## When Docker isn't running

Ferrodock shows a "Docker isn't running" screen and keeps checking in the
background. As soon as the engine is up, it connects on its own.
