//! A vsock listener: the host reaches the VM on a port number, with no
//! network involved.

use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

use super::Stream;

pub struct Listener(OwnedFd);

/// One connection from the host.
pub struct Connection(File);

/// Listens on `port` for connections from the host (any CID).
pub fn listen(port: u32) -> std::io::Result<Listener> {
    // SAFETY: plain socket calls; the descriptor is owned right away.
    unsafe {
        let fd = libc::socket(libc::AF_VSOCK, libc::SOCK_STREAM | libc::SOCK_CLOEXEC, 0);
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        let owned = OwnedFd::from_raw_fd(fd);
        let mut address: libc::sockaddr_vm = std::mem::zeroed();
        address.svm_family = libc::AF_VSOCK as libc::sa_family_t;
        address.svm_port = port;
        address.svm_cid = libc::VMADDR_CID_ANY;
        let bound = libc::bind(
            fd,
            (&raw const address).cast::<libc::sockaddr>(),
            std::mem::size_of::<libc::sockaddr_vm>() as libc::socklen_t,
        );
        if bound < 0 || libc::listen(fd, 64) < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(Listener(owned))
    }
}

impl Listener {
    pub fn accept(&self) -> std::io::Result<Connection> {
        // SAFETY: accepting on our own listening socket.
        let fd = unsafe {
            libc::accept4(
                self.0.as_raw_fd(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                libc::SOCK_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: a fresh descriptor nobody else owns.
        Ok(Connection(unsafe { File::from_raw_fd(fd) }))
    }
}

impl Read for Connection {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0.read(buf)
    }
}

impl Write for Connection {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Stream for Connection {
    fn split(&self) -> std::io::Result<Box<dyn Stream>> {
        Ok(Box::new(Connection(self.0.try_clone()?)))
    }
    fn close_write(&self) {
        // SAFETY: shutting down our own socket.
        unsafe {
            libc::shutdown(self.0.as_raw_fd(), libc::SHUT_WR);
        }
    }
}
