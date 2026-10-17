---
sidebar_position: 6
title: Contributing
---

# Contributing

Bug reports, ideas and pull requests are welcome. Open an issue on
[GitHub](https://github.com/manimovassagh/Ferrodock/issues/new/choose).

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
- Title pull requests with [Conventional Commits](https://www.conventionalcommits.org)
  (`feat: …`, `fix: …`). PRs are squash-merged, and the title becomes the
  changelog entry.

## Releases

Releases are automated. release-plz keeps a release PR open with the next
version and changelog. Merging it tags the release, builds every platform,
attaches the binaries and checksums, and publishes it. See
[RELEASING.md](https://github.com/manimovassagh/Ferrodock/blob/main/RELEASING.md).

See [CONTRIBUTING.md](https://github.com/manimovassagh/Ferrodock/blob/main/CONTRIBUTING.md)
for the full guide.

## Building these docs

```bash
cd website
npm ci
npm start        # live preview at http://localhost:3000/Ferrodock/
npm run build    # static site in website/build
```
