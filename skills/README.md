# Skills

Instructions written for AI agents, in the
[Agent Skills](https://agentskills.io) format: one folder per skill, with a
`SKILL.md` that starts with its `name` and a `description` saying when to
use it.

| Skill | What it does |
| --- | --- |
| [install-ferrodock](install-ferrodock/SKILL.md) | Installs Ferrodock from its official release, checking the download, on macOS, Windows or Linux |
| [connect-ferrodock-mcp](connect-ferrodock-mcp/SKILL.md) | Connects an AI tool to Ferrodock's MCP server, so it can manage containers |

## Using them

**Without installing anything:** the docs site serves every skill at
`https://manimovassagh.github.io/Ferrodock/skills/<name>.md`, and the
install skill also at `/install.md`. Give your AI assistant the link:

> Install Ferrodock for me, following https://manimovassagh.github.io/Ferrodock/install.md

**As skills in your tool:** copy a skill's folder where your tool loads
skills from, for example `~/.claude/skills/` for Claude Code. Then asking
"install Ferrodock" or "connect my AI to Ferrodock" is enough.

## Adding a skill

1. A new folder `skills/<name>/` with a `SKILL.md`: front matter with
   `name` (the folder's name) and `description` (what it does and when to
   use it), then the instructions.
2. Write for an agent helping a non-technical person: say what each step
   does, ask before anything that needs a password or changes a security
   setting, and use only official sources.
3. Add it to the table above. The docs site publishes it by itself (see the
   `skills` plugin in `website/docusaurus.config.js`).
