---
name: connect-ferrodock-mcp
description: Connect an AI tool (Claude Code, Codex, Gemini CLI, GitHub Copilot in VS Code, Cursor, Windsurf, Claude Desktop) to Ferrodock's MCP server, ferrodock-mcp, so it can see and manage the user's Docker containers. Use when the user asks to connect their AI tool to Ferrodock or Docker, or after installing Ferrodock.
---

# Connect an AI tool to Ferrodock

Ferrodock comes with `ferrodock-mcp`, an [MCP](https://modelcontextprotocol.io)
server. Once an AI tool has it, the tool can list containers, read logs,
start and stop services, run Quick start templates and clean up, on the
same Docker engine the Ferrodock app uses. Explain this in one sentence
and ask the person whether they want it before changing any settings.

## Rules

- **Never overwrite** an existing config file: read it, add the
  `ferrodock` entry next to what's there, keep everything else, and make a
  backup copy first (`<file>.bak`).
- Offer **read-only** mode (`--read-only`): the AI can look but not change
  anything. Recommend it when the person is unsure.
- Ferrodock's tools that change something ask for confirmation through the
  AI tool; don't promise otherwise.

## 1. Find ferrodock-mcp

```bash
command -v ferrodock-mcp        # macOS and Linux
```
```powershell
(Get-Command ferrodock-mcp -ErrorAction SilentlyContinue).Source   # Windows
```

- **Found:** use that full path below. Apps opened from the Dock or Start
  menu (Cursor, Windsurf, Claude Desktop) often don't see the terminal's
  PATH, so config files get the full path.
- **Not found, Ferrodock installed:**
  - macOS: `/Applications/Ferrodock.app/Contents/MacOS/ferrodock-mcp`
    (or **CLI & MCP → Install fdock command** in Ferrodock's sidebar);
  - Windows: `%LOCALAPPDATA%\Programs\Ferrodock\ferrodock-mcp.exe`, or next
    to wherever `Ferrodock.exe` is;
  - Linux: next to the `ferrodock` program, e.g.
    `~/.local/opt/ferrodock/ferrodock-mcp`.
  Ferrodock also puts it on the PATH the first time it starts (Windows and
  Linux), so opening Ferrodock once may be enough.
- **Ferrodock not installed:** install it first with the
  **install-ferrodock** skill:
  https://manimovassagh.github.io/Ferrodock/skills/install-ferrodock.md

Check it runs: `ferrodock-mcp --help` (or the full path).

## 2. Add it to the AI tool

Use the person's tool. `<server>` is `ferrodock-mcp`, or its full path from
step 1; add ` --read-only` after it if they chose read-only.

**Tools with a command:**

| Tool | Command |
| --- | --- |
| Claude Code | `claude mcp add ferrodock -- <server>` |
| Codex | `codex mcp add ferrodock -- <server>` |
| Gemini CLI | `gemini mcp add ferrodock <server>` |
| VS Code (GitHub Copilot) | `code --add-mcp '{"name":"ferrodock","command":"<server>"}'` (with `--read-only`: add `"args":["--read-only"]`) |

**Tools with a config file:** add this under `mcpServers`, using the full
path:

```json
{
  "mcpServers": {
    "ferrodock": { "command": "/full/path/to/ferrodock-mcp", "args": [] }
  }
}
```

| Tool | File |
| --- | --- |
| Cursor | `~/.cursor/mcp.json` |
| Windsurf | `~/.codeium/windsurf/mcp_config.json` |
| Claude Desktop, macOS | `~/Library/Application Support/Claude/claude_desktop_config.json` |
| Claude Desktop, Windows | `%APPDATA%\Claude\claude_desktop_config.json` |
| Claude Desktop, Linux | `~/.config/Claude/claude_desktop_config.json` |

Windows paths in JSON need doubled backslashes
(`C:\\Users\\…\\ferrodock-mcp.exe`). With read-only, `"args": ["--read-only"]`.

Other MCP clients take the same: a server named `ferrodock` that runs
`ferrodock-mcp`.

## 3. Check it

Tell the person to restart the AI tool (config-file tools only read their
config at start). Then check, for example by listing their containers
through the new `ferrodock` tools. If the engine isn't running, the
`engine_start` tool starts it, or they can open Ferrodock.

More: https://manimovassagh.github.io/Ferrodock/docs/cli-and-mcp
