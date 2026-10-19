---
sidebar_position: 4
title: Compose
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

To start a project that isn't running yet, paste the path of its
`compose.yaml` in the box at the top of the page and click **Up**, or drop
the file on the window. The output of `docker compose` shows while it
works. Projects you started this way are listed under **Recent**, for
one-click restarts.

## What it needs

**Up**, **Rebuild**, **Pull & recreate** and the project's merged **Logs**
use the `docker compose` command, so they need the Docker CLI with its
Compose plugin installed. Ferrodock points it at the engine you picked.
Everything else on this page (start, stop, restart, remove) works without
it.
