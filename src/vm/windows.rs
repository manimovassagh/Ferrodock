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
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

use super::wsl::{decode, linux_path, pem};
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

/// Where each connection's data from the engine goes; None once the
/// engine has sent everything.
type Connections = Arc<Mutex<HashMap<u32, UnboundedSender<Option<Vec<u8>>>>>>;

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
    to_agent: mpsc::Sender<Vec<u8>>,
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

        let (to_agent, frames) = mpsc::channel::<Vec<u8>>();
        thread::spawn(move || {
            use std::io::Write;
            for frame in frames {
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
                        let _ = to_agent.send(Vec::new());
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

    /// Stops the engine, giving its containers time to shut down, then
    /// the distro.
    pub fn stop(&self) {
        let _ = self.to_agent.send(Vec::new());
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
    to_agent: mpsc::Sender<Vec<u8>>,
    connections: Connections,
) {
    static NEXT: AtomicU32 = AtomicU32::new(1);
    loop {
        let connected = server.connect().await.is_ok();
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
async fn serve(
    pipe: NamedPipeServer,
    id: u32,
    to_agent: mpsc::Sender<Vec<u8>>,
    connections: Connections,
) {
    let (inbox, mut messages) = unbounded_channel();
    if let Ok(mut map) = connections.lock() {
        map.insert(id, inbox);
    }
    let _ = to_agent.send(mux::encode(id, mux::OPEN, b""));
    let (mut reader, mut writer) = tokio::io::split(pipe);
    let up = async {
        let mut buffer = vec![0u8; 64 * 1024];
        loop {
            match reader.read(&mut buffer).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if to_agent
                        .send(mux::encode(id, mux::DATA, &buffer[..n]))
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
    let _ = to_agent.send(mux::encode(id, mux::CLOSE, b""));
}
