//! The engine's Linux VM, through Apple's Virtualization.framework: the
//! kernel and read-only root from the engine image, a data disk, the
//! user's home folder over virtiofs, NAT networking and a vsock device
//! the Docker socket travels over. On Apple Silicon also Rosetta, so
//! x86_64 (amd64) images run too.
//!
//! Every call on the VM happens on its own dispatch queue, as the
//! framework requires; [`Machine`] only ever touches it from there.

use std::io::Write;
use std::os::fd::{FromRawFd, IntoRawFd};
use std::sync::mpsc;

use block2::RcBlock;
use dispatch2::{DispatchQueue, DispatchRetained};
use objc2::rc::Retained;
use objc2::runtime::AnyClass;
use objc2::{AnyThread, ClassType};
use objc2_foundation::{NSArray, NSError, NSFileHandle, NSString, NSURL};
use objc2_virtualization::{
    VZDirectorySharingDeviceConfiguration, VZDiskImageStorageDeviceAttachment,
    VZEntropyDeviceConfiguration, VZFileHandleSerialPortAttachment, VZLinuxBootLoader,
    VZLinuxRosettaAvailability, VZLinuxRosettaDirectoryShare, VZMACAddress,
    VZMemoryBalloonDeviceConfiguration, VZNATNetworkDeviceAttachment, VZNetworkDeviceConfiguration,
    VZSerialPortConfiguration, VZSharedDirectory, VZSingleDirectoryShare,
    VZSocketDeviceConfiguration, VZStorageDeviceConfiguration, VZVirtioBlockDeviceConfiguration,
    VZVirtioConsoleDeviceSerialPortConfiguration, VZVirtioEntropyDeviceConfiguration,
    VZVirtioFileSystemDeviceConfiguration, VZVirtioNetworkDeviceConfiguration,
    VZVirtioSocketConnection, VZVirtioSocketDevice, VZVirtioSocketDeviceConfiguration,
    VZVirtioTraditionalMemoryBalloonDeviceConfiguration, VZVirtualMachine,
    VZVirtualMachineConfiguration, VZVirtualMachineState,
};

use crate::vm::{Paths, ROSETTA_TAG, Resources, SHARE_TAG, VSOCK_PORT};

/// What the kernel is told: the read-only root, the agent as the first
/// process, the home folder shared at the same path inside, and whether
/// Rosetta is shared.
fn command_line(home: &str, rosetta: bool) -> String {
    let mut line = format!(
        "console=hvc0 root=/dev/vda ro rootfstype=ext4 init=/sbin/ferrodock-agent \
         ferrodock.data=/dev/vdb ferrodock.share={SHARE_TAG}:{home}"
    );
    if rosetta {
        line.push_str(&format!(" ferrodock.rosetta={ROSETTA_TAG}"));
    }
    line
}

/// Rosetta, shared with the guest so it runs x86_64 programs (Apple
/// Silicon, macOS 13 or newer, once it's installed), or why it can't be,
/// for the engine's log.
///
/// When it isn't installed the engine starts without it, rather than
/// bring up macOS's installer on its own: the log says how to install it.
fn rosetta() -> Result<Retained<VZLinuxRosettaDirectoryShare>, String> {
    // Before macOS 13 the class isn't there at all (and naming it would
    // panic).
    if AnyClass::get(c"VZLinuxRosettaDirectoryShare").is_none() {
        return Err("Rosetta needs macOS 13 or newer: x86_64 (amd64) images won't run".into());
    }
    match unsafe { VZLinuxRosettaDirectoryShare::availability() } {
        VZLinuxRosettaAvailability::Installed => unsafe {
            VZLinuxRosettaDirectoryShare::initWithError(VZLinuxRosettaDirectoryShare::alloc())
        }
        .map_err(|e| format!("Rosetta: {}", describe(&e))),
        VZLinuxRosettaAvailability::NotInstalled => Err(
            "Rosetta isn't installed, so x86_64 (amd64) images won't run. To install it, \
             run `softwareupdate --install-rosetta --agree-to-license`, then restart the engine"
                .into(),
        ),
        _ => Err("Rosetta isn't available: x86_64 (amd64) images won't run".into()),
    }
}

fn url(path: &std::path::Path) -> Retained<NSURL> {
    NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()))
}

fn describe(error: &NSError) -> String {
    error.localizedDescription().to_string()
}

/// The VM. It lives on its dispatch queue; this handle only sends work
/// there.
pub struct Machine {
    queue: DispatchRetained<DispatchQueue>,
    vm: Retained<VZVirtualMachine>,
}

// SAFETY: the VM object is only used on `queue` (every method below goes
// through `on_queue`), which is what the framework requires.
unsafe impl Send for Machine {}
unsafe impl Sync for Machine {}

impl Machine {
    /// Builds the configuration and checks it. Doesn't start anything.
    pub fn new(paths: &Paths, resources: &Resources) -> Result<Self, String> {
        let config = configuration(paths, resources)?;
        let queue = DispatchQueue::new("dev.ferrodock.engine", None);
        let vm = unsafe {
            VZVirtualMachine::initWithConfiguration_queue(
                VZVirtualMachine::alloc(),
                &config,
                &queue,
            )
        };
        Ok(Self { queue, vm })
    }

    /// Runs `work` on the VM's queue and waits for its result.
    fn on_queue<R: Send + 'static>(
        &self,
        work: impl FnOnce(&VZVirtualMachine) -> R + Send + 'static,
    ) -> R {
        let (tx, rx) = mpsc::channel();
        let vm = Shared(self.vm.clone());
        self.queue.exec_async(move || {
            let vm = vm;
            let _ = tx.send(work(&vm.0));
        });
        rx.recv().expect("the VM queue dropped a result")
    }

    /// Starts the VM; returns once it runs (or failed to).
    pub fn start(&self) -> Result<(), String> {
        let (tx, rx) = mpsc::channel::<Result<(), String>>();
        self.on_queue(move |vm| {
            let done = RcBlock::new(move |error: *mut NSError| {
                let result = match unsafe { error.as_ref() } {
                    Some(error) => Err(describe(error)),
                    None => Ok(()),
                };
                let _ = tx.send(result);
            });
            unsafe { vm.startWithCompletionHandler(&done) };
        });
        rx.recv().map_err(|_| "the VM never answered".to_owned())?
    }

    /// Asks the guest to shut down; returns at once.
    pub fn request_stop(&self) {
        self.on_queue(|vm| unsafe {
            if vm.canRequestStop() {
                let _ = vm.requestStopWithError();
            }
        });
    }

    /// Stops it hard (the guest gets no say).
    pub fn force_stop(&self) {
        let (tx, rx) = mpsc::channel::<()>();
        self.on_queue(move |vm| {
            let stopped = tx.clone();
            let done = RcBlock::new(move |_error: *mut NSError| {
                let _ = stopped.send(());
            });
            unsafe {
                if vm.canStop() {
                    vm.stopWithCompletionHandler(&done);
                } else {
                    let _ = tx.send(());
                }
            }
        });
        let _ = rx.recv_timeout(std::time::Duration::from_secs(10));
    }

    pub fn running(&self) -> bool {
        self.on_queue(|vm| unsafe { vm.state() } == VZVirtualMachineState::Running)
    }

    /// Opens a connection to the engine's socket inside the VM. Returns
    /// a descriptor this side owns.
    pub fn connect(&self) -> Result<std::os::unix::net::UnixStream, String> {
        let (tx, rx) = mpsc::channel::<Result<i32, String>>();
        self.on_queue(move |vm| {
            let devices = unsafe { vm.socketDevices() };
            let Some(device) = devices.firstObject() else {
                let _ = tx.send(Err("the VM has no socket device".into()));
                return;
            };
            // SAFETY: the only socket device configured is a virtio one.
            let device: Retained<VZVirtioSocketDevice> =
                unsafe { Retained::cast_unchecked(device) };
            let done = RcBlock::new(
                move |connection: *mut VZVirtioSocketConnection, error: *mut NSError| {
                    let result = match unsafe { (connection.as_ref(), error.as_ref()) } {
                        (Some(connection), _) => {
                            // The connection closes its descriptor when it goes,
                            // so keep a copy of our own.
                            let fd = unsafe { libc::dup(connection.fileDescriptor()) };
                            if fd < 0 {
                                Err(std::io::Error::last_os_error().to_string())
                            } else {
                                Ok(fd)
                            }
                        }
                        (None, Some(error)) => Err(describe(error)),
                        (None, None) => Err("no connection".into()),
                    };
                    let _ = tx.send(result);
                },
            );
            unsafe { device.connectToPort_completionHandler(VSOCK_PORT, &done) };
        });
        let fd = rx
            .recv()
            .map_err(|_| "the VM never answered".to_owned())??;
        // SAFETY: a fresh descriptor nobody else owns.
        Ok(unsafe { std::os::unix::net::UnixStream::from_raw_fd(fd) })
    }
}

/// The VM handle, moved onto its queue.
struct Shared(Retained<VZVirtualMachine>);
// SAFETY: only dereferenced on the VM's queue.
unsafe impl Send for Shared {}

fn configuration(
    paths: &Paths,
    resources: &Resources,
) -> Result<Retained<VZVirtualMachineConfiguration>, String> {
    let home = std::env::var("HOME").map_err(|_| "no home folder".to_owned())?;
    // An Intel Mac runs x86_64 programs as they are.
    let rosetta = cfg!(target_arch = "aarch64").then(rosetta);
    unsafe {
        let config = VZVirtualMachineConfiguration::new();
        config.setCPUCount(resources.cpus as usize);
        config.setMemorySize(resources.memory_mib * 1024 * 1024);

        let boot =
            VZLinuxBootLoader::initWithKernelURL(VZLinuxBootLoader::alloc(), &url(&paths.kernel));
        boot.setCommandLine(&NSString::from_str(&command_line(
            &home,
            matches!(rosetta, Some(Ok(_))),
        )));
        config.setBootLoader(Some(boot.as_super()));

        // vda: the read-only root; vdb: the data disk.
        let mut disks: Vec<Retained<VZStorageDeviceConfiguration>> = Vec::new();
        for (path, read_only) in [(&paths.rootfs, true), (&paths.data, false)] {
            let attachment = VZDiskImageStorageDeviceAttachment::initWithURL_readOnly_error(
                VZDiskImageStorageDeviceAttachment::alloc(),
                &url(path),
                read_only,
            )
            .map_err(|e| format!("{}: {}", path.display(), describe(&e)))?;
            let disk = VZVirtioBlockDeviceConfiguration::initWithAttachment(
                VZVirtioBlockDeviceConfiguration::alloc(),
                attachment.as_super(),
            );
            disks.push(Retained::into_super(disk));
        }
        config.setStorageDevices(&NSArray::from_retained_slice(&disks));

        let network = VZVirtioNetworkDeviceConfiguration::new();
        network.setAttachment(Some(VZNATNetworkDeviceAttachment::new().as_super()));
        // A card address of our own, to find the VM's address by (see
        // `vm::ports`).
        if let Some(mac) = VZMACAddress::initWithString(
            VZMACAddress::alloc(),
            &NSString::from_str(&crate::vm::ports::mac_address(&paths.dir)),
        ) {
            network.setMACAddress(&mac);
        }
        let networks: Vec<Retained<VZNetworkDeviceConfiguration>> =
            vec![Retained::into_super(network)];
        config.setNetworkDevices(&NSArray::from_retained_slice(&networks));

        // The console goes to a file: the engine's log.
        let mut log = std::fs::File::create(&paths.console)
            .map_err(|e| format!("{}: {e}", paths.console.display()))?;
        if let Some(Err(why)) = &rosetta {
            let _ = writeln!(log, "ferrodock: {why}");
        }
        let handle = NSFileHandle::initWithFileDescriptor_closeOnDealloc(
            NSFileHandle::alloc(),
            log.into_raw_fd(),
            true,
        );
        let console = VZVirtioConsoleDeviceSerialPortConfiguration::new();
        let attachment =
            VZFileHandleSerialPortAttachment::initWithFileHandleForReading_fileHandleForWriting(
                VZFileHandleSerialPortAttachment::alloc(),
                None,
                Some(&handle),
            );
        console.setAttachment(Some(attachment.as_super()));
        let ports: Vec<Retained<VZSerialPortConfiguration>> = vec![Retained::into_super(console)];
        config.setSerialPorts(&NSArray::from_retained_slice(&ports));

        let entropy: Vec<Retained<VZEntropyDeviceConfiguration>> = vec![Retained::into_super(
            VZVirtioEntropyDeviceConfiguration::new(),
        )];
        config.setEntropyDevices(&NSArray::from_retained_slice(&entropy));

        // Memory the guest frees goes back to macOS.
        let balloon: Vec<Retained<VZMemoryBalloonDeviceConfiguration>> =
            vec![Retained::into_super(
                VZVirtioTraditionalMemoryBalloonDeviceConfiguration::new(),
            )];
        config.setMemoryBalloonDevices(&NSArray::from_retained_slice(&balloon));

        let sockets: Vec<Retained<VZSocketDeviceConfiguration>> = vec![Retained::into_super(
            VZVirtioSocketDeviceConfiguration::new(),
        )];
        config.setSocketDevices(&NSArray::from_retained_slice(&sockets));

        // The home folder, at the same path inside, so bind mounts work.
        let directory = VZSharedDirectory::initWithURL_readOnly(
            VZSharedDirectory::alloc(),
            &url(std::path::Path::new(&home)),
            false,
        );
        let share =
            VZSingleDirectoryShare::initWithDirectory(VZSingleDirectoryShare::alloc(), &directory);
        let fs = VZVirtioFileSystemDeviceConfiguration::initWithTag(
            VZVirtioFileSystemDeviceConfiguration::alloc(),
            &NSString::from_str(SHARE_TAG),
        );
        fs.setShare(Some(share.as_super()));
        let mut shares: Vec<Retained<VZDirectorySharingDeviceConfiguration>> =
            vec![Retained::into_super(fs)];
        if let Some(Ok(share)) = &rosetta {
            let fs = VZVirtioFileSystemDeviceConfiguration::initWithTag(
                VZVirtioFileSystemDeviceConfiguration::alloc(),
                &NSString::from_str(ROSETTA_TAG),
            );
            fs.setShare(Some(share.as_super()));
            shares.push(Retained::into_super(fs));
        }
        config.setDirectorySharingDevices(&NSArray::from_retained_slice(&shares));

        config
            .validateWithError()
            .map_err(|e| format!("The engine's VM setup is invalid: {}", describe(&e)))?;
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn tells_the_kernel_where_everything_is() {
        let line = super::command_line("/Users/me", false);
        assert!(line.contains("init=/sbin/ferrodock-agent"));
        assert!(line.contains("ferrodock.share=home:/Users/me"));
        assert!(line.contains("root=/dev/vda ro"));
        assert!(!line.contains("ferrodock.rosetta"));
        let line = super::command_line("/Users/me", true);
        assert!(line.ends_with(" ferrodock.rosetta=rosetta"));
    }
}
