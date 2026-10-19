//! Carries the engine's socket to the host: every connection on the vsock
//! port (or the test TCP port) is joined to the engine's Unix socket.

mod vsock;

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener};
use std::os::unix::net::UnixStream;
use std::thread;

use crate::config::{Config, VSOCK_PORT};
use crate::engine::SOCKET;
use crate::log;

/// A connection that can be read and written from two threads and closed
/// one way.
pub trait Stream: Read + Write + Send + 'static {
    fn split(&self) -> std::io::Result<Box<dyn Stream>>;
    fn close_write(&self);
}

impl Stream for UnixStream {
    fn split(&self) -> std::io::Result<Box<dyn Stream>> {
        Ok(Box::new(self.try_clone()?))
    }
    fn close_write(&self) {
        let _ = self.shutdown(Shutdown::Write);
    }
}

impl Stream for std::net::TcpStream {
    fn split(&self) -> std::io::Result<Box<dyn Stream>> {
        Ok(Box::new(self.try_clone()?))
    }
    fn close_write(&self) {
        let _ = self.shutdown(Shutdown::Write);
    }
}

/// Copies both ways until both sides are done.
pub fn join(a: Box<dyn Stream>, b: Box<dyn Stream>) -> std::io::Result<()> {
    let (mut a_read, mut b_read) = (a.split()?, b.split()?);
    let (mut a_write, mut b_write) = (a, b);
    let up = thread::spawn(move || {
        let _ = std::io::copy(&mut a_read, &mut b_write);
        b_write.close_write();
    });
    let _ = std::io::copy(&mut b_read, &mut a_write);
    a_write.close_write();
    let _ = up.join();
    Ok(())
}

fn to_engine(client: Box<dyn Stream>) {
    thread::spawn(move || match UnixStream::connect(SOCKET) {
        Ok(engine) => {
            let _ = join(client, Box::new(engine));
        }
        Err(e) => log(&format!("engine socket: {e}")),
    });
}

/// Starts the listeners in the background.
pub fn serve(config: &Config) {
    match vsock::listen(VSOCK_PORT) {
        Ok(listener) => {
            log(&format!("engine offered on vsock port {VSOCK_PORT}"));
            thread::spawn(move || {
                loop {
                    match listener.accept() {
                        Ok(stream) => to_engine(Box::new(stream)),
                        Err(e) => log(&format!("vsock: {e}")),
                    }
                }
            });
        }
        Err(e) => log(&format!("vsock: {e}")),
    }
    if let Some(port) = config.tcp_port {
        match TcpListener::bind(("0.0.0.0", port)) {
            Ok(listener) => {
                log(&format!("engine offered on tcp port {port}"));
                thread::spawn(move || {
                    for stream in listener.incoming().flatten() {
                        to_engine(Box::new(stream));
                    }
                });
            }
            Err(e) => log(&format!("tcp {port}: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Unix socket "engine" that echoes in upper case, reached through
    /// a TCP client joined to it.
    #[test]
    fn joins_two_streams_both_ways() {
        let dir = std::env::temp_dir().join(format!("ferrodock-agent-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("engine.sock");
        let engine = std::os::unix::net::UnixListener::bind(&path).unwrap();
        thread::spawn(move || {
            let (mut s, _) = engine.accept().unwrap();
            let mut text = String::new();
            s.read_to_string(&mut text).unwrap();
            s.write_all(text.to_uppercase().as_bytes()).unwrap();
        });

        let front = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = front.local_addr().unwrap();
        let path2 = path.clone();
        thread::spawn(move || {
            let (client, _) = front.accept().unwrap();
            let engine = UnixStream::connect(&path2).unwrap();
            join(Box::new(client), Box::new(engine)).unwrap();
        });

        let mut client = std::net::TcpStream::connect(address).unwrap();
        client.write_all(b"ping").unwrap();
        client.shutdown(Shutdown::Write).unwrap();
        let mut reply = String::new();
        client.read_to_string(&mut reply).unwrap();
        assert_eq!(reply, "PING");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
