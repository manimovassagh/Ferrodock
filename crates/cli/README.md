# fdock

[Ferrodock](https://github.com/manimovassagh/Ferrodock) on the command line:
containers, images, logs and Quick start templates, on Ferrodock's own
Docker engine or any other.

```bash
fdock ps                      # running containers
fdock quickstart postgres     # a Postgres for local development; prints how to connect
fdock logs db -n 50 -f        # follow its logs
fdock stop db                 # start, stop, restart, pause, unpause, rm
fdock images | volumes | networks | projects | df
fdock clean images build-cache  # free space; also containers, volumes, all…
fdock compose up ./shop       # also down, restart, logs [-f]
fdock engine                  # which engine is used
fdock engine start            # start it, and wait until it answers
fdock reset --yes             # remove everything (--keep-volumes keeps them)
```

Every command takes `--json` for compact, stable JSON, cheap for scripts
and AI tools to read, and `--engine NAME` to pick an engine. By default
fdock uses `DOCKER_HOST` when set, else Ferrodock's own engine when it
runs, else your current Docker context.

Install: `cargo install ferrodock-cli`, or use the `fdock` that ships with
Ferrodock. MIT licensed.
