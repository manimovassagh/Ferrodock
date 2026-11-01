# Working on Ferrodock

## Every feature updates its docs

A pull request that adds or changes a feature also updates, in the same
pull request and without being asked:

- `README.md`: Highlights, and How it compares when it applies.
- `website/docs/`: the page for that part of the app (or a new one), and
  `intro.md` when the feature is new.
- `website/src/pages/index.js`: the landing page's features or showcase
  for anything new and visible.
- Screenshots when the UI changed (`src/app/screenshots.rs`).

## Code layout

Keep the structure flat and modular, so the code reads easily and stays
easy to maintain:

- One folder per feature in each layer, e.g. `crates/core/src/ci/`,
  `src/app/pages/ci/`, with a store file (`src/app/store/ci.rs`) and a
  worker file (`src/worker/ci.rs`) when it needs them.
- Inside a feature folder, flat files by responsibility (`workflow.rs`,
  `runner.rs`, `run_view.rs`…). No folders inside folders.
- A `mod.rs` starts with a short note on what each file does.

## Wording

- Ferrodock is "the native container manager, written in Rust". Never call
  it a Docker Desktop.
- No full stops in headlines or titles.
- Compare with other tools on facts, never on their pricing.

## Checks before pushing

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

For the website: `cd website && npm ci && npm run build`.
