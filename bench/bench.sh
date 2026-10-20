#!/usr/bin/env bash
# Measures a Docker desktop app the same way for every tool, so the
# numbers can be compared: idle memory and CPU of the app with its engine,
# time until the engine answers, and how fast a container starts.
#
# Usage: bench/bench.sh <name> <process pattern> [start command]
#   name             what to call it in the results (e.g. "Ferrodock")
#   process pattern  a regular expression matching all of the tool's
#                    processes, the app and its VM or helpers
#                    (see bench/README.md for each tool's)
#   start command    how to open the tool; give it to measure the cold
#                    start (quit the tool first)
#
# Prints one JSON line; bench/README.md explains every number.
# Needs: bash, ps, awk, the docker CLI pointed at the tool's engine.
set -euo pipefail

name=$1 pattern=$2 start=${3:-}
idle_wait=${IDLE_WAIT:-120}
samples=${SAMPLES:-60}
runs=${RUNS:-20}

now() { python3 -c 'import time; print(time.time())'; }

# Sum of resident memory (MiB) of every process whose command line
# matches the pattern.
memory() {
    ps -axo rss=,command= | awk -v p="$pattern" '
        $0 ~ p && $0 !~ /bench\.sh|awk/ { rss += $1 }
        END { printf "%.0f\n", rss / 1024 }'
}

# CPU time (seconds) those processes have used so far. ps's own %cpu is
# a lifetime average on Linux, so idle CPU comes from the difference of
# two of these over a known time instead.
cpu_seconds() {
    ps -axo time=,command= | awk -v p="$pattern" '
        $0 ~ p && $0 !~ /bench\.sh|awk/ {
            t = $1; days = 0
            if (index(t, "-")) { split(t, d, "-"); days = d[1]; t = d[2] }
            n = split(t, f, ":"); s = 0
            for (i = 1; i <= n; i++) s = s * 60 + f[i]
            total += days * 86400 + s
        }
        END { printf "%.2f\n", total }'
}

cold_start=null
if [ -n "$start" ]; then
    began=$(now)
    sh -c "$start" >/dev/null 2>&1 &
    until docker info >/dev/null 2>&1; do sleep 0.2; done
    cold_start=$(python3 -c "print(round($(now) - $began, 1))")
fi
docker info >/dev/null 2>&1 || { echo "the engine doesn't answer: is $name running?" >&2; exit 1; }

# Containers left over would skew idle numbers.
if [ -n "$(docker ps -q)" ]; then
    echo "stop all containers first: idle means no containers" >&2
    exit 1
fi

sleep "$idle_wait"
mem_total=0
cpu_before=$(cpu_seconds)
began=$(now)
for _ in $(seq "$samples"); do
    mem_total=$((mem_total + $(memory)))
    sleep 1
done
idle_mem=$((mem_total / samples))
idle_cpu=$(python3 -c "print(round(($(cpu_seconds) - $cpu_before) / ($(now) - $began) * 100, 1))")

docker image inspect alpine:3.20 >/dev/null 2>&1 || docker pull -q alpine:3.20 >/dev/null
times=()
for _ in $(seq "$runs"); do
    began=$(now)
    docker run --rm alpine:3.20 true
    times+=("$(python3 -c "print(round(($(now) - $began) * 1000))")")
done
run_ms=$(printf '%s\n' "${times[@]}" | sort -n | awk '{ a[NR] = $1 } END { print a[int((NR + 1) / 2)] }')

printf '{"tool":"%s","idle_memory_mib":%s,"idle_cpu_percent":%s,"cold_start_s":%s,"docker_run_ms":%s,"os":"%s","date":"%s"}\n' \
    "$name" "$idle_mem" "$idle_cpu" "$cold_start" "$run_ms" "$(uname -sm)" "$(date -u +%F)"
