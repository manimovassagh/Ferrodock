# Measures a Docker desktop app on Windows the same way bench.sh does on
# macOS and Linux, so the numbers can be compared: idle memory and CPU of
# the app with its engine, time until the engine answers, how fast a
# container starts, bind-mount speed and install size.
#
# Usage: bench\bench.ps1 -Name <name> -Pattern <process pattern> [-Start <command>]
#   -Name      what to call it in the results (e.g. "Ferrodock")
#   -Pattern   a regular expression matching all of the tool's processes,
#              the app and its WSL helpers (see bench/README.md)
#   -Start     how to open the tool; give it to measure the cold start
#              (quit the tool first)
#   -AppPath   the installed app's folder, for its size on disk
#   -Download  the downloaded .zip or installer, for its size
#
# Prints one JSON line; bench/README.md explains every number.
param(
    [Parameter(Mandatory)] [string] $Name,
    [Parameter(Mandatory)] [string] $Pattern,
    [string] $Start = "",
    [string] $AppPath = "",
    [string] $Download = "",
    [int] $IdleWait = $(if ($env:IDLE_WAIT) { [int]$env:IDLE_WAIT } else { 120 }),
    [int] $Samples = $(if ($env:SAMPLES) { [int]$env:SAMPLES } else { 60 }),
    [int] $Runs = $(if ($env:RUNS) { [int]$env:RUNS } else { 20 })
)
$ErrorActionPreference = "Stop"

function Get-ToolProcesses {
    Get-Process | Where-Object { $_.ProcessName -match $Pattern -or ($_.Path -and $_.Path -match $Pattern) }
}

# The WSL VM (vmmem / vmmemWSL) holds every distro's memory, the engine's
# among them, so it counts when the pattern names it.
function Get-MemoryMiB { [math]::Round(((Get-ToolProcesses | Measure-Object WorkingSet64 -Sum).Sum) / 1MB) }
function Get-CpuSeconds { (Get-ToolProcesses | ForEach-Object { $_.TotalProcessorTime.TotalSeconds } | Measure-Object -Sum).Sum }

function Test-Engine { docker info *> $null; $LASTEXITCODE -eq 0 }

$coldStart = $null
if ($Start) {
    $watch = [Diagnostics.Stopwatch]::StartNew()
    Start-Process -FilePath "cmd.exe" -ArgumentList "/c", $Start -WindowStyle Hidden
    while (-not (Test-Engine)) { Start-Sleep -Milliseconds 200 }
    $coldStart = [math]::Round($watch.Elapsed.TotalSeconds, 1)
}
if (-not (Test-Engine)) { throw "the engine doesn't answer: is $Name running?" }
if (docker ps -q) { throw "stop all containers first: idle means no containers" }

Start-Sleep -Seconds $IdleWait
$memory = 0
$cpuBefore = Get-CpuSeconds
$watch = [Diagnostics.Stopwatch]::StartNew()
for ($i = 0; $i -lt $Samples; $i++) {
    $memory += Get-MemoryMiB
    Start-Sleep -Seconds 1
}
$idleMemory = [math]::Round($memory / $Samples)
$idleCpu = [math]::Round(((Get-CpuSeconds) - $cpuBefore) / $watch.Elapsed.TotalSeconds * 100, 1)

docker image inspect alpine:3.20 *> $null
if ($LASTEXITCODE -ne 0) { docker pull -q alpine:3.20 | Out-Null }
$times = for ($i = 0; $i -lt $Runs; $i++) {
    $watch = [Diagnostics.Stopwatch]::StartNew()
    docker run --rm alpine:3.20 true
    [math]::Round($watch.Elapsed.TotalMilliseconds)
}
$sorted = $times | Sort-Object
$runMs = $sorted[[math]::Floor(($sorted.Count - 1) / 2)]

# Bind mounts: a folder of the user's profile, written from a container.
$mount = Join-Path $env:USERPROFILE (".ferrodock-bench-" + [guid]::NewGuid().ToString("N").Substring(0, 8))
New-Item -ItemType Directory $mount | Out-Null
try {
    $watch = [Diagnostics.Stopwatch]::StartNew()
    docker run --rm -v "${mount}:/w" alpine:3.20 sh -c 'dd if=/dev/zero of=/w/big bs=1M count=256 conv=fsync 2>/dev/null'
    $mountWrite = [math]::Round(256 / $watch.Elapsed.TotalSeconds)
    $watch = [Diagnostics.Stopwatch]::StartNew()
    docker run --rm -v "${mount}:/w" alpine:3.20 sh -c 'mkdir /w/small && i=0; while [ $i -lt 2000 ]; do echo $i > /w/small/$i; i=$((i + 1)); done'
    $smallFiles = [math]::Round($watch.Elapsed.TotalSeconds, 2)
}
finally {
    Remove-Item -Recurse -Force $mount
}

function Get-SizeMiB([string] $Path) {
    if (-not $Path -or -not (Test-Path $Path)) { return $null }
    [math]::Round(((Get-ChildItem -Recurse -File $Path | Measure-Object Length -Sum).Sum) / 1MB)
}

[ordered]@{
    tool                = $Name
    idle_memory_mib     = $idleMemory
    idle_cpu_percent    = $idleCpu
    cold_start_s        = $coldStart
    docker_run_ms       = $runMs
    mount_write_mib_s   = $mountWrite
    mount_small_files_s = $smallFiles
    app_size_mib        = Get-SizeMiB $AppPath
    download_mib        = Get-SizeMiB $Download
    os                  = "Windows " + [Environment]::OSVersion.Version
    date                = (Get-Date).ToUniversalTime().ToString("yyyy-MM-dd")
} | ConvertTo-Json -Compress
