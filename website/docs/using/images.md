---
sidebar_position: 5
title: Images
description: "Pull, search, tag, push, save, load and run Docker images in Ferrodock."
---

# Images

The **Images** page lists your images with their size and the containers
that use them.

## Pull and search

Type an image name such as `nginx:alpine` in the **Pull** box. Docker Hub
results appear by themselves once you pause typing, official images first,
with their stars.

- **Pull** on a result pulls it; pressing **Enter** pulls exactly what you
  typed. A progress bar shows the download.
- **Run…** next to a result opens the Run dialog for that image; the image
  is pulled when you run it.

## Run a container

To start a container from an image, hover its row and click **▶**, or
select it and click **Run…**. The dialog asks for:

- **Name** (optional; Docker picks one otherwise).
- **Ports**, filled in from the ports the image exposes. Change the host
  side if it's taken.
- **Environment** variables, **Volumes** (a volume name or a host path),
  **Network**, **Restart** policy and an optional **Command**.

The equivalent `docker run` command is shown as you type, with a **Copy**
button, so you can reuse it in scripts.

## Build an image

**Build…** at the top of the page builds an image from a folder with a
Dockerfile. Pick the folder, then check:

- **Dockerfile**, if it has another name or lives in a subfolder.
- **Name**, made from the folder's name (`my-app:latest`).
- **Build arguments**, one `KEY=value` per line, and the **Target stage**
  for a multi-stage Dockerfile.
- **Don't use the build cache** to run every step again.

The build's output streams above the list, and the new image appears when
it's done. Files your `.dockerignore` excludes aren't sent.

Ferrodock uses the engine's own builder, so no Docker CLI is needed. That
builder doesn't support BuildKit-only Dockerfile features, such as
`RUN --mount=type=cache`, heredocs or a `# syntax=` line; for those, use
`docker build` (or `docker buildx build`) in a terminal.

## Tag, push, save and load

In an image's details, the **⋯** menu has:

- **Tag…** gives it another name, such as `ghcr.io/me/app:1.0`.
- **Push** uploads it, with progress, to the registry its name starts with.
- **Save as .tar** writes it to your Downloads folder.

To load a saved image, drop its `.tar` on the window while the Images page
is open.

Pulling and pushing use the Docker CLI's logins: whatever `docker login`
stored in `~/.docker/config.json`, including credential helpers such as the
macOS keychain. Private registries work the same way.

## History

An image's details pane has a **History** section listing the layers it
was built from, with the biggest ones highlighted, so you can see where its
size comes from.

## Removing images

- **Remove** in the details pane deletes one image. It's disabled, with the
  reason on hover, while a container still uses it.
- **Remove unused** in the page header removes every image no container
  uses.
- Tick rows (or **Select all** in the header) to remove several at once.
  Ones still in use are kept, and the question says so.

To free space across images, containers, volumes and the build cache in one
go, use the [Disk](./disk.md) page.
