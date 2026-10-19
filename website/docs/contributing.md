---
sidebar_position: 9
title: Contributing
---

# Contributing

Ferrodock is open source, and bug reports, ideas and pull requests are
welcome.

- **Found a bug or have an idea?**
  [Open an issue](https://github.com/manimovassagh/Ferrodock/issues/new/choose).
  For bugs, include your OS, your Docker engine and the Ferrodock version.
- **Want to change something?** The
  [contributing guide](https://github.com/manimovassagh/Ferrodock/blob/main/CONTRIBUTING.md)
  covers the development setup, the checks CI runs, the code layout and how
  to title pull requests.
- **Found a security issue?** Please report it privately, as described in
  [SECURITY.md](https://github.com/manimovassagh/Ferrodock/blob/main/SECURITY.md).

To work on Ferrodock you need [Rust](https://rustup.rs) 1.95 or newer:

```bash
git clone https://github.com/manimovassagh/Ferrodock.git
cd Ferrodock
cargo run
```

## Improving these docs

The site lives in the `website/` folder of the repository, and every page
has an **Edit this page** link at the bottom. To preview your changes:

```bash
cd website
npm ci
npm start        # live preview at http://localhost:3000/Ferrodock/
```
