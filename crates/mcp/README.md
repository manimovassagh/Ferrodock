# ferrodock-mcp

An [MCP](https://modelcontextprotocol.io) server that lets AI tools see and
manage Docker, from [Ferrodock](https://github.com/manimovassagh/Ferrodock):
containers, logs, images, volumes, networks, Compose projects, disk usage,
and popular services (Postgres, Redis…) started for local development in
one step. Answers are compact JSON.

```bash
claude mcp add ferrodock -- ferrodock-mcp              # Claude Code
claude mcp add ferrodock -- ferrodock-mcp --read-only  # only tools that look
```

Other MCP clients take the same command: `ferrodock-mcp`, over standard
input and output.

| Looks | Changes things (asks first) |
|---|---|
| `engine_status`, `list_containers`, `container_logs`, `inspect_container`, `list_images`, `list_volumes`, `list_networks`, `list_projects`, `disk_usage`, `list_quickstart_templates` | `container_action`, `remove_container` (destructive), `pull_image`, `run_quickstart` |

`--engine NAME` picks the engine. By default: `DOCKER_HOST` when set, else
Ferrodock's own engine when it runs, else your current Docker context.

Install: `cargo install ferrodock-mcp`, or use the one that ships with
Ferrodock. MIT licensed.
