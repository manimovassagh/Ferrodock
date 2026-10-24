---
sidebar_position: 4
title: Compose
description: "Run, rebuild, stop and follow Docker Compose projects in Ferrodock, with or without Docker Compose installed."
---

# Compose

The **Compose** page groups containers started by Docker Compose by
project. A project shows **Running**, **Running 2/3** (some services
stopped) or **Stopped**.

## Running projects

- **Start all**, **Stop all** and **Restart all** act on the whole project.
- **Remove** deletes the project's containers once it's stopped. Volumes,
  networks and images are kept, and `docker compose up` recreates
  everything.
- **Rebuild** next to a service (in Overview) rebuilds and recreates just
  that service. **Rebuild all** does every service, and **Pull & recreate**
  pulls newer images and recreates the containers.
- **Logs** shows every service's logs together, live, each service in its
  own colour. Click a service to see only its lines.

## Starting a project

To start a project that isn't running yet, click **Open…** at the top of
the page and pick its `compose.yaml`, paste the file's path in the box and
click **Up**, or drop the file on the window. **Open a Compose file…** in
the command palette (⌘K, Ctrl+K on Windows and Linux) does the same. The output of `docker compose` shows while it
works. Projects you started this way are listed under **Recent**, for
one-click restarts.

## What it needs

**Up**, **Rebuild**, **Pull & recreate** and the project's merged **Logs**
run Docker Compose, pointed at the engine you picked. Nothing needs to be
installed for them:

- When the `docker` command is installed, Ferrodock uses its
  `docker compose`.
- When it isn't (with only [Ferrodock's own engine](../engine.md), for
  example), the first of these downloads Docker Compose once, the official
  standalone build from its GitHub releases, checked against its SHA-256.
  It's kept in Ferrodock's folder (`~/.ferrodock/bin` on macOS and Linux,
  `%LOCALAPPDATA%\Ferrodock\bin` on Windows) and used from then on.
  Docker Compose is free and open source (Apache-2.0).

Everything else on this page (start, stop, restart, remove) doesn't need
Compose at all.
