# Benchmarks

`bench.sh` measures a Docker desktop app the same way for every tool, so
the numbers can be compared. It prints one JSON line per run; the results
on [the comparison page](https://manimovassagh.github.io/Ferrodock/compare)
come from `website/src/data/benchmarks.json`, made from these lines.

| Number | What it is |
| --- | --- |
| `idle_memory_mib` | Resident memory of the app and its engine (VM, helpers), averaged over a minute, two minutes after start, with no containers |
| `idle_cpu_percent` | CPU they use in that minute, as a share of one core |
| `cold_start_s` | Seconds from opening the app until `docker info` answers (only with a start command) |
| `docker_run_ms` | Median time of `docker run --rm alpine:3.20 true`, over 20 runs |

## Running it

1. Same machine for every tool, nothing else heavy running, on power.
2. Install only one tool at a time, or quit the others completely
   (menu bar icon → Quit), and make sure `docker` talks to the one you
   measure: `docker context use …` or `DOCKER_HOST`.
3. Quit the tool, then run:

```bash
bench/bench.sh "Ferrodock"        'Ferrodock|com.apple.Virtualization.VirtualMachine' 'open -a Ferrodock'
bench/bench.sh "Docker Desktop"   'Docker|com.docker|com.apple.Virtualization.VirtualMachine' 'open -a Docker'
bench/bench.sh "OrbStack"         'OrbStack|orbstack|com.apple.Virtualization.VirtualMachine' 'open -a OrbStack'
bench/bench.sh "Rancher Desktop"  'Rancher Desktop|lima|qemu|com.apple.Virtualization.VirtualMachine' 'open -a "Rancher Desktop"'
bench/bench.sh "Podman Desktop"   'Podman Desktop|podman|vfkit|gvproxy|krunkit' 'open -a "Podman Desktop"'
```

Each pattern matches the tool's own processes and the virtual machine it
runs (`com.apple.Virtualization.VirtualMachine` is the process macOS runs
every Virtualization-framework VM in, which is why only one tool may run
at a time). Leave out the start command to measure an engine that's
already running; `cold_start_s` is then `null`.

Settings change the numbers (memory given to the VM, CPUs), so measure
each tool with its defaults and say so. `IDLE_WAIT`, `SAMPLES` and `RUNS`
in the environment change how long it waits and how much it samples.

## Adding results

Add each line to `website/src/data/benchmarks.json` under `runs`, with the
machine in `machine` (for example "MacBook Pro M3 Pro, 18 GB, macOS 15.6").
The page charts whatever is there, per machine.
