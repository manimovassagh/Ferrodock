---
sidebar_position: 5.5
title: Command line and AI tools
description: "Use Ferrodock from a terminal with fdock, and let AI tools such as Claude see and manage your containers through its MCP server."
---

# Command line and AI tools

Ferrodock comes with two more programs, built on the same core as the app:

- **`fdock`**: Ferrodock on the command line.
- **`ferrodock-mcp`**: an [MCP](https://modelcontextprotocol.io) server, so
  AI tools such as Claude can see and manage your containers.

In the app, **CLI & MCP** at the bottom of the sidebar shows the commands
for this computer, ready to copy, including the MCP setup for your AI tool.

Both use the same engine as Ferrodock: `DOCKER_HOST` when it's set, else
Ferrodock's own engine when it runs, else your current Docker context. Pick
another with `--engine NAME`; `fdock engine` lists them. The `docker`
command itself can use Ferrodock's engine with `docker context use
ferrodock` (see [Ferrodock's own engine](./engine.md)).

## Getting them

Both come with every download of Ferrodock. To use them from any terminal,
they need to be on your PATH:

- **Windows and Linux**: Ferrodock puts them there itself the first time it
  starts. On Windows it adds its folder to your user Path; on Linux it links
  them into `~/.local/bin`. Open a new terminal afterwards.
- **macOS**: click **Install fdock command** under **CLI & MCP** in the
  sidebar. macOS asks for your password once, to link them into
  `/usr/local/bin`. (When that folder is writable without a password,
  Ferrodock does it by itself on first start.)
- **Homebrew** (`brew install --cask ferrodock`) puts both on your PATH
  already.
- **With Cargo**: `cargo install ferrodock-cli ferrodock-mcp`.

To do it by hand instead, **CLI & MCP → Or do it yourself** shows the
commands for your computer. For example, on macOS:

```bash
sudo ln -sf /Applications/Ferrodock.app/Contents/MacOS/fdock /usr/local/bin/fdock
sudo ln -sf /Applications/Ferrodock.app/Contents/MacOS/ferrodock-mcp /usr/local/bin/ferrodock-mcp
```

## fdock

```bash
fdock ps                         # running containers (-a: all)
fdock logs web -n 50 -f          # the last 50 lines, then follow
fdock stop web                   # also start, restart, pause, unpause
fdock rm web -f                  # remove; -f stops it first
fdock inspect web                # image, environment, ports, mounts, health
fdock images                     # also volumes, networks, projects
fdock pull nginx:alpine
fdock build . -t my-app:1.0 --build-arg VERSION=1.0
fdock df                         # disk usage
fdock clean images build-cache   # free space; see below
fdock compose up ./shop          # also down, restart, logs [-f]
fdock engine                     # the engines, and which one is used
fdock engine start               # start it, and wait until it answers
fdock reset --yes                # remove everything; see below
```

### Compose

`fdock compose up`, `down`, `restart` and `logs` take a Compose file, or the
folder it's in (by default the current one), and run Docker Compose on the
engine `fdock` uses. That's the `docker compose` command when there is one;
without it (as with only Ferrodock's own engine), `fdock` downloads
Ferrodock's copy of Compose once, the same one the app uses. `up` starts the
services in the background; `logs -f` keeps printing new output.

### Cleaning up

`fdock clean` frees disk space and says how much: `containers` (stopped
ones), `images` (every image no container uses), `dangling-images`
(untagged ones), `volumes` (no container mounts them; their data goes too),
`networks`, `build-cache`, or `all`: everything but volumes, like
`docker system prune -a`. Name several at once: `fdock clean containers
volumes`.

`fdock reset` removes every container, image, network and volume, and the
build cache, for a clean start; `--keep-volumes` keeps the volumes and
their data. It never asks: without `--yes` it only says what it would
remove, so a script can't reset by accident.

### Starting the engine

`fdock engine start` starts the engine `fdock` would use (or the one given
with `--engine`), unless one answers already, and waits until it answers.
Another engine starts the way Ferrodock starts it, such as the `docker`
service on Linux. Ferrodock's own engine runs inside the app on macOS and
Windows, so `fdock` opens Ferrodock in the background, which starts it (with
**Start the Docker engine when Ferrodock opens** on, as it is by default);
on Linux it starts the engine's service directly.

### Quick start

`fdock quickstart` lists the [Quick start](./using/quick-start.md)
templates; `fdock quickstart NAME` runs one, with a free name and ports, and
prints how to connect:

```console
$ fdock quickstart postgres
Started postgres (postgres:17)
postgres://postgres:postgres@localhost:5432/app
```

Change what you need: `--name db`, `--version 16`, `--port 6543` (once per
port), `--set POSTGRES_PASSWORD=secret`, or `--no-data` to keep no volume.

### JSON for scripts and AI tools

Every command takes `--json` and prints one line of compact JSON. Sizes and
times are human-readable, and empty fields are left out, so it stays short:

```console
$ fdock ps --json
[{"name":"db","id":"bbabe46e0d1a","image":"postgres:17","state":"running","status":"Up 4 seconds","ports":["6543->5432/tcp"]}]
```

When something fails, the message goes to standard error and `fdock` exits
with status 1.

## ferrodock-mcp

`ferrodock-mcp` speaks MCP over standard input and output. Add it to your AI
tool once:

| Tool | Setup |
| --- | --- |
| Claude Code | `claude mcp add ferrodock -- ferrodock-mcp` |
| Codex | `codex mcp add ferrodock -- ferrodock-mcp` |
| GitHub Copilot (VS Code) | `code --add-mcp '{"name":"ferrodock","command":"ferrodock-mcp"}'` |
| Gemini CLI | `gemini mcp add ferrodock ferrodock-mcp` |
| Cursor | the JSON below in `~/.cursor/mcp.json` |
| Windsurf | the JSON below in `~/.codeium/windsurf/mcp_config.json` |
| Claude Desktop | the JSON below in `claude_desktop_config.json` |

```json
{
  "mcpServers": {
    "ferrodock": { "command": "ferrodock-mcp" }
  }
}
```

Other MCP clients take the same: a server named `ferrodock` that runs
`ferrodock-mcp`. **CLI & MCP** in the app's sidebar shows each of these
with the right path for your computer, ready to copy.

Give the full path if it isn't on the PATH, such as
`/Applications/Ferrodock.app/Contents/MacOS/ferrodock-mcp` or
`C:\Tools\Ferrodock\ferrodock-mcp.exe`.

### Tools

| Looks | Changes things |
|---|---|
| `engine_status`, `list_containers`, `container_logs`, `inspect_container`, `list_images`, `list_volumes`, `list_networks`, `list_projects`, `disk_usage`, `list_quickstart_templates` | `container_action` (start, stop, restart, pause, resume), `remove_container`, `pull_image`, `build_image`, `run_quickstart`, `engine_start`, `compose_up`, `compose_down`, `clean`, `reset` |

Tools that change something are marked that way, so your AI tool asks before
it uses them; `remove_container`, `compose_down`, `clean` and `reset` are
marked destructive. `reset` also does nothing unless it's called with
`confirm: true`, and says it's only for when you've asked for it.
`compose_up` and `compose_down` take a Compose file or its folder, as an
absolute path. Environment values that look like secrets (passwords,
tokens, keys) are hidden in `inspect_container` unless asked for.

To let an AI tool look but never change anything, start the server with
`--read-only`: the tools that change things aren't offered at all, and
secret-looking environment values (passwords, tokens) always stay hidden.

```bash
claude mcp add ferrodock -- ferrodock-mcp --read-only
```
