---
sidebar_position: 5
title: Connecting to other engines
---

# Connecting to other engines

Ferrodock works with any Docker engine: [its own](./engine.md), the Docker
engine on your Linux machine, or an engine somewhere else on your network.
It talks to it through the Docker Engine API, the same way the `docker`
command does.

## Which engine Ferrodock uses

Ferrodock connects to the same engine as the `docker` command:

1. The engine you last picked in Ferrodock, if it's still there.
2. Otherwise your current Docker context (`docker context use …`, or the
   `DOCKER_CONTEXT` variable).
3. Otherwise `DOCKER_HOST`, or the platform's default:

   | Platform | Default |
   |---|---|
   | macOS, Linux | `unix:///var/run/docker.sock` |
   | Windows | `npipe:////./pipe/docker_engine` |

## Switching engines

Click the engine's status at the bottom of the sidebar (for example
**● Docker 29.8**) to see every engine Ferrodock knows about, and pick one.
The switch happens right away, without restarting, and Ferrodock remembers
it.

The list has:

- **default**: `DOCKER_HOST`, or the platform's default shown above.
- Every **Docker context** (`docker context ls`), whether it uses a local
  socket, a remote `tcp://` host with TLS, or `ssh://`.
- **Ferrodock**, its own engine, once it's set up. The `ferrodock` context
  that points at it isn't listed twice; when it's your current context,
  Ferrodock's engine is the one picked.
- **Local engine**: a Docker socket in `~/.docker/run`, if there is one and
  no context points at it already.

Compose actions use the `docker compose` command, and Ferrodock points it
at the engine you picked.

## Using `DOCKER_HOST`

Set `DOCKER_HOST` before starting Ferrodock to change the **default** entry:

```bash
# A socket in another place
DOCKER_HOST=unix://$HOME/.docker/run/docker.sock ferrodock

# The macOS app, from a terminal
DOCKER_HOST=unix://$HOME/.docker/run/docker.sock \
  /Applications/Ferrodock.app/Contents/MacOS/ferrodock
```

## Remote engines

The easiest way to reach an engine on another machine is a Docker context,
which then shows up in Ferrodock's engine list.

**Over SSH** (macOS and Linux):

```bash
docker context create build-box --docker "host=ssh://me@build-box.local"
```

Your SSH login needs to work without a password prompt (for example with a
key loaded in `ssh-agent`), and your user on that machine needs access to
Docker.

**Over TCP with TLS:**

```bash
docker context create remote --docker \
  "host=tcp://203.0.113.10:2376,ca=ca.pem,cert=cert.pem,key=key.pem"
```

Ferrodock uses the certificates stored with the context. Plain `tcp://`
without TLS works too, but only use it on a network you trust.

When `DOCKER_HOST` points at a remote `tcp://` or `ssh://` engine, port
links open on that machine's address instead of `localhost`.

## When Docker isn't running

If the engine isn't reachable, Ferrodock shows **Docker isn't running** and
keeps checking every few seconds. As soon as the engine is up, it connects
on its own.

When Ferrodock opens and the engine isn't running, it also tries to start
it for you ("Starting the Docker engine…"):

- **macOS**: the app that runs your engine. Ferrodock recognises it from
  the engine's Docker context or socket, or remembers it from last time.
- **Linux**: the `docker` service (it may ask for your password).
- **Ferrodock's own engine**: Ferrodock boots it.

If the engine stops later, the screen has a **Start** button that does the
same. To turn the automatic start off, go to
[Settings → Startup](./using/settings.md#startup).

## Setting up an engine

If Ferrodock finds no engine at all:

- **Apple Silicon Mac, Windows or Linux**: click **Set up Ferrodock's
  engine**. See [Ferrodock's own engine](./engine.md).
- **Intel Mac**: run a Docker engine locally, or on another machine and
  connect to it [remotely](#remote-engines).
