//! The engine on Windows: a WSL distro called `ferrodock`, with the Docker
//! engine and `ferrodock-agent` inside.
//!
//! Ferrodock starts the agent with `wsl.exe` and talks to it over that
//! process's standard input and output: private to Ferrodock, with no
//! network port for a firewall or another program to get in the way. Each
//! client of the named pipes below becomes one connection on that stream
//! (see [`mux`]), which the agent joins to the engine's socket.

use std::collections::HashMap;
use std::os::windows::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use tokio::sync::mpsc::{self, UnboundedSender, unbounded_channel};

use super::wsl::{Wsl, decode, linux_path, pem, readiness};
use super::{Paths, Resources, download, mux};

const DISTRO: &str = "ferrodock";
const AGENT: &str = "/sbin/ferrodock-agent";
/// Runs wsl.exe without a console window popping up.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// Ferrodock's own pipe, then the Docker CLI's default one, taken only
/// when nothing else serves it, so `docker` works with no setup.
const PIPES: [&str; 2] = [r"\\.\pipe\ferrodock_engine", r"\\.\pipe\docker_engine"];
/// How long the engine gets to stop its containers.
const GRACE: Duration = Duration::from_secs(20);
/// The administrator prompt was dismissed (ERROR_CANCELLED).
const CANCELLED: i32 = 1223;
/// Installed, but only works after a restart (ERROR_SUCCESS_REBOOT_REQUIRED).
const REBOOT: i32 = 3010;

/// Where each connection's data from the engine goes; None once the
/// engine has sent everything.
type Connections = Arc<Mutex<HashMap<u32, UnboundedSender<Option<Vec<u8>>>>>>;

/// Frames on their way to the agent, at most [`QUEUED`] of them: a client
/// sending faster than WSL takes it in (a `docker load`, a big build
/// context) waits, instead of the frames piling up in memory.
type ToAgent = mpsc::Sender<Vec<u8>>;

/// Frames of up to 64 KiB each, so 16 MiB at most.
const QUEUED: usize = 256;

/// Runs `wsl.exe` with `args` and returns what it printed.
fn wsl(args: &[&str]) -> Result<String, String> {
    let output = Command::new("wsl.exe")
        .args(args)
        .env("WSL_UTF8", "1")
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("WSL isn't available ({e}). Turn it on with `wsl --install`."))?;
    let text = format!("{}{}", decode(&output.stdout), decode(&output.stderr));
    if output.status.success() {
        Ok(text)
    } else {
        Err(format!("wsl {}: {}", args.join(" "), text.trim()))
    }
}

/// How WSL stands on this machine.
fn wsl_state() -> Result<Wsl, String> {
    let run = |arg: &str| {
        Command::new("wsl.exe")
            .arg(arg)
            .env("WSL_UTF8", "1")
            .stdin(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .output()
    };
    let version = run("--version").map_err(|e| {
        format!(
            "WSL isn't available ({e}). Ferrodock's engine needs Windows 10 version 2004 \
             or later, or Windows 11."
        )
    })?;
    let status = run("--status").map_err(|e| e.to_string())?;
    let text = format!("{}{}", decode(&status.stdout), decode(&status.stderr));
    Ok(readiness(
        version.status.success(),
        (status.status.success(), &text),
    ))
}

/// Runs `wsl.exe` with `args` as administrator, after Windows' prompt
/// (UAC), and returns its exit code.
fn wsl_as_admin(args: &str) -> Result<i32, String> {
    // Start-Process fails when the prompt is dismissed; why is somewhere
    // down the exception's causes.
    let script = format!(
        "try {{ $p = Start-Process wsl.exe -ArgumentList '{args}' -Verb RunAs \
         -WindowStyle Hidden -Wait -PassThru; exit $p.ExitCode }} \
         catch {{ $e = $_.Exception; while ($e) {{ \
         if ($e.NativeErrorCode -eq {CANCELLED}) {{ exit {CANCELLED} }}; \
         $e = $e.InnerException }}; [Console]::Error.WriteLine($_); exit 1 }}"
    );
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("Couldn't ask for administrator approval: {e}"))?;
    match output.status.code() {
        Some(code @ (0 | CANCELLED | REBOOT)) => Ok(code),
        code => Err(format!(
            "`wsl {args}` failed ({}). Try it in a terminal opened as administrator. {}",
            code.map_or_else(|| "no exit code".into(), |c| format!("exit code {c}")),
            decode(&output.stderr).trim()
        )),
    }
}

/// Makes sure WSL can run the engine's distro, installing or updating it
/// as administrator when needed (`installing` is called first). Returns
/// false when WSL is installed but works only after Windows restarts.
pub fn prepare(installing: impl Fn()) -> Result<bool, String> {
    let args = match wsl_state()? {
        Wsl::Ready => return Ok(true),
        Wsl::Missing => "--install --no-distribution",
        Wsl::Outdated => "--update",
    };
    installing();
    match wsl_as_admin(args)? {
        CANCELLED => Err("Setup needs administrator approval to install WSL.".into()),
        REBOOT => Ok(false),
        // Turning WSL on usually takes a restart too, even when the
        // installer doesn't say so.
        _ => Ok(wsl_state()? == Wsl::Ready),
    }
}

/// Imports the downloaded image into WSL as the `ferrodock` distro.
pub fn install(paths: &Paths, archive: &[u8]) -> Result<(), String> {
    let tar = paths.dir.join("ferrodock-engine.tar");
    download::decompress(archive, &tar)?;
    // What an earlier setup that didn't finish left behind.
    let _ = wsl(&["--unregister", DISTRO]);
    let result = std::fs::create_dir_all(&paths.distro)
        .map_err(|e| format!("{}: {e}", paths.distro.display()))
        .and_then(|()| {
            wsl(&[
                "--import",
                DISTRO,
                &paths.distro.to_string_lossy(),
                &tar.to_string_lossy(),
                "--version",
                "2",
            ])
        });
    let _ = std::fs::remove_file(&tar);
    result.map(drop)
}

/// Unregisters the distro, which deletes its disk: every image, container
/// and volume. The next start downloads and imports it again.
pub fn reset(paths: &Paths) -> Result<(), String> {
    // Fails when it isn't registered (a reset that didn't finish): fine,
    // as long as its disk is gone.
    let unregistered = wsl(&["--unregister", DISTRO]);
    if paths.installed() {
        return unregistered.map(drop);
    }
    Ok(())
}

/// Run as root in the distro with the new image's tar: puts the image's
/// files in place of the old ones, and keeps the engine's data
/// (`/var/lib/docker`: images, containers and volumes) and what WSL
/// provides.
const UPDATE: &str = r#"set -eu -o pipefail
archive=$1 new=/var/lib/ferrodock-update
rm -rf "$new"
mkdir -p "$new"
trap 'rm -rf "$new"' EXIT
# Unpacked apart first, so a damaged image changes nothing.
tar -x -f "$archive" -C "$new"
rm -rf "$new/dev" "$new/proc" "$new/sys" "$new/run" "$new/tmp" "$new/mnt" \
    "$new/var/lib/docker" "$new/var/lib/containerd"
# A folder that became a link can't be replaced in place.
for entry in "$new"/*; do
    name=${entry#"$new"/}
    if [ -L "$entry" ] && [ -d "/$name" ] && [ ! -L "/$name" ]; then
        echo "/$name is a link in the new image" >&2
        exit 1
    fi
done
# tar writes each file anew rather than over the old one, so the
# programs doing this keep running.
tar -c -C "$new" . | tar -x -C /
# The agent adds Windows' certificates to the new image's bundle.
rm -f /etc/ssl/certs/ca-certificates.crt.orig
"#;

/// Puts a newer image's files into the `ferrodock` distro, keeping its
/// images, containers and volumes. The distro holds both, so it isn't
/// imported again.
pub fn update(paths: &Paths, archive: &[u8]) -> Result<(), String> {
    let tar = paths.dir.join("ferrodock-engine.tar");
    let script = paths.dir.join("update-engine.sh");
    download::decompress(archive, &tar)?;
    let result = std::fs::write(&script, UPDATE)
        .map_err(|e| format!("{}: {e}", script.display()))
        .and_then(|()| {
            let (Some(script), Some(tar)) = (linux_path(&script), linux_path(&tar)) else {
                return Err(format!("WSL can't reach {}", paths.dir.display()));
            };
            // The engine is stopped while its programs change.
            let _ = wsl(&["--terminate", DISTRO]);
            wsl(&[
                "-d", DISTRO, "-u", "root", "--exec", "/bin/sh", &script, &tar,
            ])
        });
    let _ = wsl(&["--terminate", DISTRO]);
    let _ = std::fs::remove_file(&script);
    let _ = std::fs::remove_file(&tar);
    result.map(drop)
}

/// Writes the certificates Windows trusts to a file the agent adds to the
/// engine's own, so pulls work behind a proxy that inspects TLS, as on
/// many company networks. Returns where the agent finds the file.
fn trusted_certificates(paths: &Paths) -> Option<String> {
    let found = rustls_native_certs::load_native_certs();
    if found.certs.is_empty() {
        return None;
    }
    let file = paths.dir.join("certs.pem");
    std::fs::write(&file, pem(found.certs.iter().map(|c| c.as_ref()))).ok()?;
    linux_path(&file)
}

/// The running engine: the agent's wsl.exe process and the named pipes.
pub struct Running {
    child: Mutex<Child>,
    /// Frames for the agent. An empty one closes its input, which stops
    /// the engine.
    to_agent: ToAgent,
    runtime: Mutex<Option<tokio::runtime::Runtime>>,
}

impl Running {
    pub fn start(paths: &Paths, _resources: &Resources) -> Result<Self, String> {
        // An agent left by a Ferrodock that didn't quit cleanly.
        let _ = wsl(&["--terminate", DISTRO]);
        let log = std::fs::File::create(&paths.console)
            .map_err(|e| format!("{}: {e}", paths.console.display()))?;
        let mut args = vec!["-d", DISTRO, "-u", "root", "--exec", AGENT, "--stdio"];
        let certs = trusted_certificates(paths);
        if let Some(certs) = &certs {
            args.extend(["--certs", certs.as_str()]);
        }
        let mut child = Command::new("wsl.exe")
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(log)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("Couldn't start the engine: {e}"))?;
        let (Some(mut input), Some(mut output)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            return Err("Couldn't start the engine".into());
        };

        let (to_agent, mut frames) = mpsc::channel::<Vec<u8>>(QUEUED);
        thread::spawn(move || {
            use std::io::Write;
            while let Some(frame) = frames.blocking_recv() {
                if frame.is_empty() || input.write_all(&frame).is_err() {
                    break;
                }
                let _ = input.flush();
            }
        });
        let connections = Connections::default();
        let dispatch = connections.clone();
        thread::spawn(move || {
            while let Ok(Some((id, kind, data))) = mux::read(&mut output) {
                let Ok(mut map) = dispatch.lock() else { break };
                match kind {
                    mux::DATA => {
                        if let Some(inbox) = map.get(&id) {
                            let _ = inbox.send(Some(data));
                        }
                    }
                    _ => {
                        if let Some(inbox) = map.remove(&id) {
                            let _ = inbox.send(None);
                        }
                    }
                }
            }
            // The agent is gone: every connection ends.
            if let Ok(mut map) = dispatch.lock() {
                map.clear();
            }
        });

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        {
            let _context = runtime.enter();
            for (i, name) in PIPES.into_iter().enumerate() {
                match ServerOptions::new().first_pipe_instance(true).create(name) {
                    Ok(server) => {
                        runtime.spawn(listen(name, server, to_agent.clone(), connections.clone()));
                    }
                    // Ferrodock's own pipe is a must; the Docker CLI's is a
                    // bonus, left to whoever has it.
                    Err(e) if i == 0 => {
                        let _ = to_agent.try_send(Vec::new());
                        let _ = child.kill();
                        return Err(format!("{name}: {e}"));
                    }
                    Err(_) => {}
                }
            }
        }
        Ok(Self {
            child: Mutex::new(child),
            to_agent,
            runtime: Mutex::new(Some(runtime)),
        })
    }

    pub fn is_running(&self) -> bool {
        self.child
            .lock()
            .map(|mut c| matches!(c.try_wait(), Ok(None)))
            .unwrap_or(false)
    }

    /// Stopped from Settings: the same as when Ferrodock quits.
    pub fn shut_down(&self) -> Result<(), String> {
        self.stop();
        Ok(())
    }

    /// Stops the engine, giving its containers time to shut down, then
    /// the distro.
    pub fn stop(&self) {
        // Never waits: with the queue full, the agent is stopped below
        // once the grace time is up.
        let _ = self.to_agent.try_send(Vec::new());
        if let Ok(mut child) = self.child.lock() {
            let waited = std::time::Instant::now();
            while matches!(child.try_wait(), Ok(None)) && waited.elapsed() < GRACE {
                thread::sleep(Duration::from_millis(100));
            }
            let _ = child.kill();
        }
        let _ = wsl(&["--terminate", DISTRO]);
        if let Some(runtime) = self.runtime.lock().ok().and_then(|mut r| r.take()) {
            runtime.shutdown_background();
        }
    }
}

/// Accepts clients on the pipe `name`, one connection to the engine each.
async fn listen(
    name: &'static str,
    mut server: NamedPipeServer,
    to_agent: ToAgent,
    connections: Connections,
) {
    static NEXT: AtomicU32 = AtomicU32::new(1);
    loop {
        let connected = server.connect().await.is_ok();
        if !connected {
            // Not straight round again: a pipe that keeps failing would
            // spin a core.
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        // The next client needs a fresh instance of the pipe.
        let next = loop {
            match ServerOptions::new().create(name) {
                Ok(next) => break next,
                Err(_) => tokio::time::sleep(Duration::from_secs(1)).await,
            }
        };
        let client = std::mem::replace(&mut server, next);
        if connected {
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            tokio::spawn(serve(client, id, to_agent.clone(), connections.clone()));
        }
    }
}

/// Carries one client's connection to the engine and back.
async fn serve(pipe: NamedPipeServer, id: u32, to_agent: ToAgent, connections: Connections) {
    let (inbox, mut messages) = unbounded_channel();
    if let Ok(mut map) = connections.lock() {
        map.insert(id, inbox);
    }
    let _ = to_agent.send(mux::encode(id, mux::OPEN, b"")).await;
    let (mut reader, mut writer) = tokio::io::split(pipe);
    let up = async {
        let mut buffer = vec![0u8; 64 * 1024];
        loop {
            match reader.read(&mut buffer).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if to_agent
                        .send(mux::encode(id, mux::DATA, &buffer[..n]))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            }
        }
    };
    let down = async {
        while let Some(Some(data)) = messages.recv().await {
            if writer.write_all(&data).await.is_err() {
                break;
            }
        }
    };
    // Either side being done ends the connection: a pipe can't be closed
    // one way only.
    tokio::select! {
        () = up => {}
        () = down => {}
    }
    if let Ok(mut map) = connections.lock() {
        map.remove(&id);
    }
    let _ = to_agent.send(mux::encode(id, mux::CLOSE, b"")).await;
}
