# Contributing to IronStack

Thanks for taking the time to contribute! Bug reports, ideas and pull
requests are all welcome.

## Reporting bugs and asking for features

Use the [issue templates](https://github.com/manimovassagh/IronStack/issues/new/choose).
For bugs, include your OS, your Docker engine (Docker Desktop, Colima, …) and
the IronStack version.

## Development setup

You need [Rust](https://rustup.rs) 1.95 or newer and a running Docker Engine.

```bash
git clone https://github.com/manimovassagh/IronStack.git
cd IronStack
cargo run
```

## Before opening a pull request

CI runs these, so running them locally saves a round trip:

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test                        # unit tests
cargo test -- --include-ignored   # also tests against your local Docker Engine
```

## Code layout

| Path | What lives there |
|---|---|
| `src/docker/` | Docker Engine API, one file per resource. No UI code. |
| `src/worker/` | Background thread that runs Docker calls and streams. |
| `src/app/store/` | Everything currently known about the engine. |
| `src/app/page.rs` | The `Page` trait every sidebar section implements. |
| `src/app/pages/` | One module per page. |
| `src/app/components/` | Reusable widgets: table, details pane, dialogs, charts. |
| `src/theme/` | Colours, widget styling and fonts. |

Some guidelines:

- Keep each module focused on one job. Pages read from the store and ask
  for actions through `Intent`s; they never talk to Docker directly.
- To add a sidebar page, implement `Page` in a new file under
  `src/app/pages/` and add it to `pages::all()`.
- Anything that deletes data must go through a confirmation (`Intent::Confirm`).
- Add a unit test for any logic that isn't pure drawing.
- Write commit messages in plain English: a short summary line, then the
  why.

## License

By contributing, you agree that your contributions are licensed under the
[MIT License](LICENSE).
