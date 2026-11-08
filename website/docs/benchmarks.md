---
sidebar_position: 9
title: Benchmarks
description: "How Ferrodock is measured against other container apps: idle memory and CPU, start time, container start, bind-mount speed and size, with scripts anyone can run."
---

# Benchmarks

Claims about speed and weight should come with numbers anyone can check.
The scripts in [`bench/`](https://github.com/manimovassagh/Ferrodock/tree/main/bench)
measure a container app the same way whichever it is: Ferrodock, Docker
Desktop, OrbStack, Podman Desktop or Rancher Desktop.

## What's measured

| Number | How |
|---|---|
| Idle memory | Resident memory of the app and its engine (VM, helpers), averaged over a minute, two minutes after start, with no containers |
| Idle CPU | CPU time they use in that minute, as a share of one core |
| Cold start | Seconds from opening the app until `docker info` answers |
| Container start | Median of 20 `docker run --rm alpine:3.20 true` |
| Bind-mount write | MiB/s writing 256 MiB to a folder of your home mounted into a container |
| Small files | Seconds to write 2,000 small files there, like `npm install` |
| Size | The installed app on disk, and the download |

## Running them

On one machine, with nothing else heavy running and on power, install or run
one tool at a time, with its default settings, and point `docker` at it.
Then, for each tool:

```bash
bench/bench.sh "Ferrodock" 'Ferrodock|com.apple.Virtualization.VirtualMachine' 'open -a Ferrodock'
```

```powershell
bench\bench.ps1 -Name Ferrodock -Pattern 'Ferrodock|vmmem|wsl' -Start 'start "" "%LOCALAPPDATA%\Ferrodock\Ferrodock.exe"'
```

Each prints one line of JSON. The
[bench README](https://github.com/manimovassagh/Ferrodock/blob/main/bench/README.md)
has the process pattern for every tool and the options.

## Results

Results are published with the machine, the versions and the date, so they
can be reproduced. Runs on an Apple Silicon Mac come first, then Windows.
