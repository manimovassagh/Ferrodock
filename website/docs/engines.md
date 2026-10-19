---
sidebar_position: 4
title: Connecting to Docker
---

# Connecting to Docker

On Apple Silicon Macs and on Windows, Ferrodock can run its own engine,
so there is nothing else to install. It also connects to any Docker
engine you already run.

Ferrodock connects to the platform's default Docker endpoint:

| Platform | Default |
|---|---|
| Windows | `npipe:////./pipe/docker_engine` |
| macOS, Linux | `unix:///var/run/docker.sock` |

To use a different engine, set `DOCKER_HOST` before starting Ferrodock:

```bash
# If your engine doesn't use the default socket
DOCKER_HOST=unix://$HOME/.docker/run/docker.sock ferrodock

# Ferrodock's own engine (Apple Silicon)
DOCKER_HOST=unix://$HOME/.ferrodock/docker.sock ferrodock
```

To launch the macOS app bundle with a custom host:

```bash
DOCKER_HOST=unix://$HOME/.docker/run/docker.sock \
  /Applications/Ferrodock.app/Contents/MacOS/ferrodock
```

## When Docker isn't running

Ferrodock shows a "Docker isn't running" screen and keeps checking in the
background. As soon as the engine is up, it connects on its own.
