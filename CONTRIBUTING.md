# Contributing to Ferrodock

Thanks for taking the time to contribute! Bug reports, ideas and pull
requests are all welcome.

## Reporting bugs and asking for features

Use the [issue templates](https://github.com/manimovassagh/Ferrodock/issues/new/choose).
For bugs, include your OS, your Docker engine (Ferrodock's engine, a native `dockerd`, …) and
the Ferrodock version.

## Development setup

You need [Rust](https://rustup.rs) 1.95 or newer and a running Docker Engine.

```bash
git clone https://github.com/manimovassagh/Ferrodock.git
cd Ferrodock
cargo run
```

## Before opening a pull request

CI runs these, so running them locally saves a round trip:

```bash
cargo fmt
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                        # unit tests
cargo test --workspace -- --include-ignored   # also tests against your local Docker Engine
cargo deny check licenses advisories          # permissive licenses, no known vulnerabilities
```

The guest agent is its own crate, outside the workspace: if you change
`agent/`, run the same with `--manifest-path agent/Cargo.toml`.

## Code layout

| Path | What lives there |
|---|---|
| `crates/core/` | `ferrodock-core`: the Docker Engine API, one file per resource. No UI code; shared with the CLI and the MCP server. |
| `crates/cli/` | `fdock`, the command line. |
| `crates/mcp/` | `ferrodock-mcp`, the MCP server for AI tools. |
| `src/vm/` | Ferrodock's own engine: the Mac VM, the Windows WSL distro, the Linux service. |
| `src/worker/` | Background thread that runs Docker calls and streams. |
| `src/app/store/` | Everything currently known about the engine. |
| `src/app/page.rs` | The `Page` trait every sidebar section implements. |
| `src/app/pages/` | One module per page. |
| `src/app/components/` | Reusable widgets: table, details pane, dialogs, charts. |
| `src/update/` | The in-app updater. |
| `src/theme/` | Colours, widget styling and fonts. |
| `agent/` | The guest agent inside the engine's Linux VM (its own crate, built for musl). |
| `engine/` | Scripts that build the engine images, published as `engine-v*` releases. |
| `skills/` | Agent Skills: instructions AI assistants follow to install and connect Ferrodock. |
| `website/` | The docs site and landing page (Docusaurus). |

Some guidelines:

- Keep each module focused on one job. Pages read from the store and ask
  for actions through `Intent`s; they never talk to Docker directly.
- To add a sidebar page, implement `Page` in a new file under
  `src/app/pages/` and add it to `pages::all()`.
- Anything that deletes data must go through a confirmation (`Intent::Confirm`).
- Add a unit test for any logic that isn't pure drawing.
- Write commit messages in plain English: a short summary line, then the
  why.

## Reviews

[CODEOWNERS](.github/CODEOWNERS) asks the maintainer for a review on every
pull request. Changes to what ships to people's machines (the engine, the
guest agent, the updater, the release workflows) get a closer look, so
explain in the description what they change and how you tested it.

## Security

Please don't report security problems in a public issue or pull request:
see [SECURITY.md](SECURITY.md) for how to report them privately.

## Pull request titles

PRs are squash-merged, so the PR title becomes the commit on `main`, and
it drives versioning and the changelog. Use
[Conventional Commits](https://www.conventionalcommits.org):

```
feat: add image pulling
fix(logs): keep the scroll position when new lines arrive
perf: skip redraws when nothing changed
docs: explain DOCKER_HOST on Windows
chore(deps): update bollard
```

`feat` and `fix` show up in the changelog as Added and Fixed. Add `!`
(`feat!: …`) for breaking changes. See [RELEASING.md](RELEASING.md) for how
releases work.

Don't edit the version in `Cargo.toml` or `CHANGELOG.md` yourself: the
release PR does that.

## License

By contributing, you agree that your contributions are licensed under the
[MIT License](LICENSE).

## Screenshots

The screenshots in the README and on the website are rendered offscreen
from the real app, against whatever your local Docker Engine is running:

```bash
SCREENSHOTS=target/screenshots cargo test screenshots -- --ignored
```

The test fails if any text uses a glyph the bundled fonts don't have, so
it also guards against icons turning into empty boxes.
