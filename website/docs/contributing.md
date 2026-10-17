---
sidebar_position: 6
title: Contributing
---

# Contributing

Bug reports, ideas and pull requests are welcome. Open an issue on
[GitHub](https://github.com/manimovassagh/IronStack/issues/new/choose).

## Checks

CI runs these on every pull request:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

`cargo test -- --include-ignored` also runs the tests that talk to your local
Docker Engine.

## Guidelines

- Keep modules focused on one job. Pages never talk to Docker directly.
- Anything that deletes data must ask for confirmation first.
- Add unit tests for logic that isn't pure drawing.
- Note user-visible changes in `CHANGELOG.md` under "Unreleased".

See [CONTRIBUTING.md](https://github.com/manimovassagh/IronStack/blob/main/CONTRIBUTING.md)
for the full guide.

## Building these docs

```bash
cd website
npm ci
npm start        # live preview at http://localhost:3000/IronStack/
npm run build    # static site in website/build
```
