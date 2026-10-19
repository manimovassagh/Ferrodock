---
sidebar_position: 5
title: Architecture
---

# Architecture

Ferrodock is 100% Rust:

- [egui / eframe](https://github.com/emilk/egui) for the native UI
- [bollard](https://github.com/fussybeaver/bollard) for the Docker Engine API
- [tokio](https://tokio.rs) for async I/O

## Layout

```
src/
├── main.rs            window setup
├── docker/            Docker Engine API, one file per resource (no UI code)
├── worker/            background thread: Docker calls, log and stats streams
├── vm/                Ferrodock's own engine: download, Apple Virtualization VM
├── update/            in-app updates from GitHub releases
├── theme/             colour palettes, widget styling, fonts
└── app/
    ├── mod.rs         wires the store, the shell and the pages together
    ├── store/         everything currently known about the engine
    ├── page.rs        the Page trait every sidebar section implements
    ├── pages/         containers · compose · images · volumes · networks
    ├── components/    table, details pane, dialogs, charts, toasts
    ├── shell.rs       shared page header and "Docker isn't running" screen
    └── sidebar.rs
agent/                 first process inside the engine's Linux VM (own crate)
engine/                builds the engine image: kernel config, root filesystem
```

Ferrodock's own engine is a small Linux VM, run through Apple's
Virtualization framework. `engine/` builds its image on CI: a kernel and a
read-only root disk with the Docker engine and `ferrodock-agent`. The agent
mounts the disks and the shared home folder, brings up the network, runs
`dockerd` and offers its socket over vsock. On the Mac, `vm/` passes
`~/.ferrodock/docker.sock` through to it.

## How data flows

1. The UI sends a **request** to the worker over a channel: refresh a list,
   run an action, or watch a container.
2. The **worker** runs it on a tokio runtime and sends **responses** back.
   The UI thread never waits on Docker.
3. Responses land in the **store**, which holds containers, projects, images,
   volumes, networks and the live logs and stats.
4. **Pages** draw from the store. To change something, a page returns an
   **intent**: run a task, or ask for confirmation first. The app turns
   intents into worker requests.

## Adding a page

Implement the `Page` trait in a new file under `src/app/pages/` and register
it in `pages::all()`. The sidebar entry, header, search box and details pane
all work through the trait, so nothing else needs to change.

```rust
pub trait Page {
    fn title(&self) -> &'static str;
    fn resource(&self) -> Resource;
    fn count(&self, store: &Store) -> usize;
    fn summary(&self, store: &Store) -> String;
    fn show(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx);
    fn has_selection(&self, store: &Store) -> bool;
    fn show_details(&mut self, ui: &mut egui::Ui, ctx: &mut PageCtx);
    fn clear_selection(&mut self);
    // Optional: search_hint, toolbar, watched_container
}
```
