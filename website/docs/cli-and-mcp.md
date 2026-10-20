---
sidebar_position: 5.5
title: Command line and AI tools
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
another with `--engine NAME`; `fdock engine` lists them.

## Getting them

- **Homebrew** (`brew install --cask ferrodock`) puts both on your PATH.
- **macOS, from the DMG**: they're inside the app. Link them onto your PATH:
  ```bash
  sudo ln -sf /Applications/Ferrodock.app/Contents/MacOS/fdock /usr/local/bin/fdock
  sudo ln -sf /Applications/Ferrodock.app/Contents/MacOS/ferrodock-mcp /usr/local/bin/ferrodock-mcp
  ```
- **Windows**: `fdock.exe` and `ferrodock-mcp.exe` are in the zip, next to
  `Ferrodock.exe`.
- **Linux**: `fdock` and `ferrodock-mcp` are in the tarball, next to
  `ferrodock`.
- **With Cargo**: `cargo install ferrodock-cli ferrodock-mcp`.

On a Mac and on Windows, Ferrodock's own engine runs while Ferrodock does,
so keep the app open (it can sit in the menu bar) when the tools use it.

## fdock

```bash
fdock ps                         # running containers (-a: all)
fdock logs web -n 50 -f          # the last 50 lines, then follow
fdock stop web                   # also start, restart, pause, unpause
fdock rm web -f                  # remove; -f stops it first
fdock inspect web                # image, environment, ports, mounts, health
fdock images                     # also volumes, networks, projects
fdock pull nginx:alpine
fdock df                         # disk usage
fdock engine                     # the engines, and which one is used
```

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

**Claude Code**

```bash
claude mcp add ferrodock -- ferrodock-mcp
```

**Claude Desktop and other MCP clients**: add a server with the command
`ferrodock-mcp`, for example:

```json
{
  "mcpServers": {
    "ferrodock": { "command": "ferrodock-mcp" }
  }
}
```

Give the full path if it isn't on the PATH, such as
`/Applications/Ferrodock.app/Contents/MacOS/ferrodock-mcp` or
`C:\Tools\Ferrodock\ferrodock-mcp.exe`.

### Tools

| Looks | Changes things |
|---|---|
| `engine_status`, `list_containers`, `container_logs`, `inspect_container`, `list_images`, `list_volumes`, `list_networks`, `list_projects`, `disk_usage`, `list_quickstart_templates` | `container_action` (start, stop, restart, pause, resume), `remove_container`, `pull_image`, `run_quickstart` |

Tools that change something are marked that way, so your AI tool asks before
it uses them; `remove_container` is marked destructive. Environment values
that look like secrets (passwords, tokens, keys) are hidden in
`inspect_container` unless asked for.

To let an AI tool look but never change anything, start the server with
`--read-only`: the tools that change things aren't offered at all.

```bash
claude mcp add ferrodock -- ferrodock-mcp --read-only
```
