//! The engine on macOS: the VM, and the Docker socket on the Mac that
//! reaches the engine inside it.

mod machine;

use std::io::{Read, Write};
use std::net::Shutdown;
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::{Arc, Mutex};
use std::thread;

pub use machine::Machine;

use super::ports::{Forwarder, mac_address};
use super::{Paths, Resources, download};

/// Size of the data disk (sparse: it only takes what's used).
const DATA_DISK: u64 = 128 * 1024 * 1024 * 1024;

/// Unpacks the downloaded image: the kernel and the root disk.
pub fn install(paths: &Paths, archive: &[u8]) -> Result<(), String> {
    download::unpack(archive, &paths.image, &["kernel", "rootfs.img"])
}

/// Puts a newer image in place of the old one. Images, containers and
/// volumes are on the data disk, apart from it, so they stay.
pub fn update(paths: &Paths, archive: &[u8]) -> Result<(), String> {
    install(paths, archive)
}

/// Copies one way, then tells the other side nothing more comes.
fn pipe(mut from: UnixStream, mut to: UnixStream) {
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        match from.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if to.write_all(&buffer[..n]).is_err() {
                    break;
                }
            }
        }
    }
    let _ = to.shutdown(Shutdown::Write);
}

/// Joins a client of the Mac's socket to a connection into the VM.
fn join(client: UnixStream, engine: UnixStream) {
    let (Ok(client_out), Ok(engine_out)) = (client.try_clone(), engine.try_clone()) else {
        return;
    };
    let up = thread::spawn(move || pipe(client, engine_out));
    pipe(engine, client_out);
    let _ = up.join();
}

/// The running engine: the VM, the socket on the Mac, and the containers'
/// ports on `localhost`.
pub struct Running {
    machine: Arc<Machine>,
    ports: Mutex<Option<Forwarder>>,
}

impl Running {
    /// Boots the VM and opens `paths.socket` for Docker clients.
    pub fn start(paths: &Paths, resources: &Resources) -> Result<Self, String> {
        if !paths.data.exists() {
            std::fs::File::create(&paths.data)
                .and_then(|f| f.set_len(DATA_DISK))
                .map_err(|e| format!("{}: {e}", paths.data.display()))?;
        }
        let machine = Arc::new(Machine::new(paths, resources)?);
        machine.start()?;
        // A socket left from a run that didn't end cleanly.
        let _ = std::fs::remove_file(&paths.socket);
        let listener = UnixListener::bind(&paths.socket)
            .map_err(|e| format!("{}: {e}", paths.socket.display()))?;
        let bridge = machine.clone();
        thread::Builder::new()
            .name("engine-socket".into())
            .spawn(move || {
                for client in listener.incoming().flatten() {
                    let machine = bridge.clone();
                    thread::spawn(move || match machine.connect() {
                        Ok(engine) => join(client, engine),
                        // The engine isn't listening yet (still booting):
                        // the client sees a closed connection and retries.
                        Err(_) => drop(client),
                    });
                }
            })
            .map_err(|e| e.to_string())?;
        let ports = Forwarder::start(paths.host(), mac_address(&paths.dir))?;
        Ok(Self {
            machine,
            ports: Mutex::new(Some(ports)),
        })
    }

    pub fn is_running(&self) -> bool {
        self.machine.running()
    }

    /// Shuts the guest down, and stops it hard if it takes too long.
    pub fn stop(&self) {
        if let Some(ports) = self.ports.lock().ok().and_then(|mut p| p.take()) {
            ports.stop();
        }
        self.machine.request_stop();
        for _ in 0..50 {
            if !self.machine.running() {
                return;
            }
            thread::sleep(std::time::Duration::from_millis(100));
        }
        self.machine.force_stop();
    }
}
