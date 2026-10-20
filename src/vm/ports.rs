//! Published ports on `localhost` (macOS). The engine runs in a VM with its
//! own address on the Mac's NAT network, so `docker run -p 8080:80` would
//! only answer at that address. Ferrodock follows the engine's containers
//! and, for every published TCP port, listens on the Mac's `localhost` and
//! passes each connection on to the same port in the VM.
//!
//! The VM's address comes from the Mac's DHCP leases
//! (`/var/db/dhcpd_leases`), found by the VM's network card, whose
//! address Ferrodock picks once and keeps in `mac-address`.

// Elsewhere only the tests use it.
#![cfg_attr(not(all(engine, target_os = "macos")), allow(dead_code))]

use std::collections::{BTreeSet, HashMap};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ferrodock_core::{ContainerInfo, DockerClient, Engine};
use futures_util::StreamExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

/// The Mac's DHCP server writes the VMs' leases here.
const LEASES: &str = "/var/db/dhcpd_leases";

/// The VM network card's address: picked at random once (a locally
/// administered one, so it can't clash with real hardware), then kept, so
/// the VM is found again in the leases.
pub fn mac_address(dir: &Path) -> String {
    let file = dir.join("mac-address");
    if let Ok(text) = std::fs::read_to_string(&file)
        && normalize(text.trim()).is_some()
    {
        return text.trim().to_owned();
    }
    let mac = random_mac();
    let _ = std::fs::write(&file, &mac);
    mac
}

fn random_mac() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos()),
    );
    hasher.write_u32(std::process::id());
    let bytes = hasher.finish().to_le_bytes();
    // Locally administered, unicast.
    let first = (bytes[0] & 0xfc) | 0x02;
    format!(
        "{first:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        bytes[1], bytes[2], bytes[3], bytes[4], bytes[5]
    )
}

/// `f2:0a:…` and `f2:a:…` (how the leases write it) both become
/// `f2:a:…`.
fn normalize(mac: &str) -> Option<String> {
    let parts: Vec<u8> = mac
        .split(':')
        .map(|p| u8::from_str_radix(p, 16).ok())
        .collect::<Option<_>>()?;
    (parts.len() == 6).then(|| {
        parts
            .iter()
            .map(|b| format!("{b:x}"))
            .collect::<Vec<_>>()
            .join(":")
    })
}

/// The address leased to the card `mac`, from the leases file's text: the
/// newest one, which the file lists first.
pub fn leased_address(leases: &str, mac: &str) -> Option<Ipv4Addr> {
    let wanted = normalize(mac)?;
    let mut ip = None;
    for line in leases.lines().map(str::trim) {
        if line == "{" {
            ip = None;
        } else if let Some(value) = line.strip_prefix("ip_address=") {
            ip = value.parse().ok();
        } else if let Some(value) = line.strip_prefix("hw_address=") {
            // `1,f2:a:…`: the hardware type, then the address.
            let hw = value.split_once(',').map_or(value, |(_, a)| a);
            if normalize(hw).as_deref() == Some(wanted.as_str())
                && let Some(ip) = ip
            {
                return Some(ip);
            }
        }
    }
    None
}

/// The TCP ports to offer on `localhost`: those published on all of the
/// VM's addresses by running containers. Ports bound to the VM's own
/// loopback can't be reached from outside it, so they're left out.
pub fn wanted(containers: &[ContainerInfo]) -> BTreeSet<u16> {
    containers
        .iter()
        .filter(|c| c.is_active())
        .flat_map(|c| &c.ports)
        .filter(|p| p.proto == "tcp" && matches!(p.ip.as_str(), "" | "0.0.0.0" | "::"))
        .filter_map(|p| p.public)
        .collect()
}

/// Where connections go: the VM's address, looked up when first needed
/// and again when it stops answering.
#[derive(Clone)]
struct Target {
    mac: String,
    ip: Arc<Mutex<Option<Ipv4Addr>>>,
}

impl Target {
    async fn connect(&self, port: u16) -> Option<TcpStream> {
        let cached = self.ip.lock().ok().and_then(|ip| *ip);
        if let Some(ip) = cached
            && let Some(stream) = attempt(ip, port).await
        {
            return Some(stream);
        }
        let text = tokio::fs::read_to_string(LEASES).await.ok()?;
        let ip = leased_address(&text, &self.mac)?;
        if let Ok(mut slot) = self.ip.lock() {
            *slot = Some(ip);
        }
        if Some(ip) == cached {
            return None;
        }
        attempt(ip, port).await
    }
}

async fn attempt(ip: Ipv4Addr, port: u16) -> Option<TcpStream> {
    tokio::time::timeout(Duration::from_secs(3), TcpStream::connect((ip, port)))
        .await
        .ok()?
        .ok()
}

/// Listens on `localhost:port` (IPv4 and IPv6) and passes connections on.
/// None when the port is taken on the Mac.
fn offer(port: u16, target: &Target) -> Option<JoinHandle<()>> {
    let listeners: Vec<TcpListener> = [Ipv4Addr::LOCALHOST.into(), Ipv6Addr::LOCALHOST.into()]
        .into_iter()
        .filter_map(|ip: std::net::IpAddr| {
            let socket = std::net::TcpListener::bind(SocketAddr::new(ip, port)).ok()?;
            socket.set_nonblocking(true).ok()?;
            TcpListener::from_std(socket).ok()
        })
        .collect();
    (!listeners.is_empty()).then(|| tokio::spawn(serve(listeners, port, target.clone())))
}

/// Passes each connection on `listeners` on to `port` in the VM.
async fn serve(listeners: Vec<TcpListener>, port: u16, target: Target) {
    let accepts = listeners.into_iter().map(|listener| {
        let target = target.clone();
        async move {
            loop {
                let Ok((mut client, _)) = listener.accept().await else {
                    continue;
                };
                let target = target.clone();
                tokio::spawn(async move {
                    if let Some(mut vm) = target.connect(port).await {
                        let _ = tokio::io::copy_bidirectional(&mut client, &mut vm).await;
                    }
                });
            }
        }
    });
    futures_util::future::join_all(accepts).await;
}

/// Follows the engine's containers and keeps one listener per published
/// port.
pub struct Forwarder {
    runtime: tokio::runtime::Runtime,
}

impl Forwarder {
    /// Starts following the engine at `host` (its socket on the Mac),
    /// whose VM has the network card `mac`.
    pub fn start(host: String, mac: String) -> Result<Self, String> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("ports")
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        let target = Target {
            mac,
            ip: Arc::default(),
        };
        runtime.spawn(follow(host, target));
        Ok(Self { runtime })
    }

    pub fn stop(self) {
        self.runtime.shutdown_background();
    }
}

async fn follow(host: String, target: Target) {
    let engine = Engine {
        name: crate::vm::NAME.into(),
        host,
        tls: None,
        current: false,
    };
    let Ok(client) = DockerClient::connect_to(&engine) else {
        return;
    };
    let mut offered: HashMap<u16, JoinHandle<()>> = HashMap::new();
    loop {
        // Ports only change when containers start and stop, so the list
        // is read again on each container event (and every half minute,
        // in case an event was missed).
        let mut events = Box::pin(client.events());
        loop {
            let Ok(containers) = client.list_containers().await else {
                break;
            };
            let ports = wanted(&containers);
            offered.retain(|port, task| {
                let keep = ports.contains(port);
                if !keep {
                    task.abort();
                }
                keep
            });
            for port in ports {
                if !offered.contains_key(&port)
                    && let Some(task) = offer(port, &target)
                {
                    offered.insert(port, task);
                }
            }
            match tokio::time::timeout(Duration::from_secs(30), events.next()).await {
                Ok(Some(Ok(_))) | Err(_) => {}
                // The engine went away (or isn't up yet).
                Ok(Some(Err(_)) | None) => break,
            }
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrodock_core::PortMapping;

    const LEASES_TEXT: &str = "{
\tname=other
\tip_address=192.168.64.2
\thw_address=1,a2:b:c:d:e:f
\tidentifier=1,a2:b:c:d:e:f
\tlease=0x6714f0a1
}
{
\tip_address=192.168.64.7
\thw_address=1,f2:a:0:1:22:3
\tidentifier=1,f2:a:0:1:22:3
\tlease=0x6714f0b2
}
{
\tip_address=192.168.64.5
\thw_address=1,f2:a:0:1:22:3
\tlease=0x6714e000
}
";

    #[test]
    fn finds_the_vm_in_the_leases() {
        assert_eq!(
            leased_address(LEASES_TEXT, "f2:0a:00:01:22:03"),
            Some(Ipv4Addr::new(192, 168, 64, 7))
        );
        assert_eq!(
            leased_address(LEASES_TEXT, "a2:0b:0c:0d:0e:0f"),
            Some(Ipv4Addr::new(192, 168, 64, 2))
        );
        assert_eq!(leased_address(LEASES_TEXT, "02:00:00:00:00:01"), None);
        assert_eq!(leased_address("", "f2:0a:00:01:22:03"), None);
    }

    #[test]
    fn picks_a_local_mac_address_once() {
        let mac = random_mac();
        assert!(normalize(&mac).is_some());
        let first = u8::from_str_radix(&mac[..2], 16).unwrap();
        assert_eq!(first & 0x03, 0x02, "locally administered, unicast");

        let dir = std::env::temp_dir().join(format!("ferrodock-mac-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let kept = mac_address(&dir);
        assert_eq!(mac_address(&dir), kept);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn container(state: &str, ports: &[(&str, Option<u16>, &str)]) -> ContainerInfo {
        ContainerInfo {
            state: state.into(),
            ports: ports
                .iter()
                .map(|(ip, public, proto)| PortMapping {
                    ip: (*ip).into(),
                    public: *public,
                    private: 80,
                    proto: (*proto).into(),
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn offers_published_tcp_ports_of_running_containers() {
        let containers = [
            container(
                "running",
                &[
                    ("0.0.0.0", Some(8080), "tcp"),
                    ("::", Some(8080), "tcp"),
                    ("0.0.0.0", Some(5353), "udp"),
                    ("127.0.0.1", Some(9000), "tcp"),
                    ("", None, "tcp"),
                ],
            ),
            container("exited", &[("0.0.0.0", Some(5432), "tcp")]),
            container("paused", &[("0.0.0.0", Some(6379), "tcp")]),
        ];
        assert_eq!(wanted(&containers), BTreeSet::from([6379, 8080]));
    }

    /// A connection reaches the VM's port: a server on 127.0.0.1 stands
    /// in for the VM.
    #[tokio::test]
    async fn passes_connections_on() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let vm = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let vm_port = vm.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut s, _) = vm.accept().await.unwrap();
            let mut buf = [0u8; 4];
            s.read_exact(&mut buf).await.unwrap();
            s.write_all(&buf.to_ascii_uppercase()).await.unwrap();
        });
        let target = Target {
            mac: "02:00:00:00:00:01".into(),
            ip: Arc::new(Mutex::new(Some(Ipv4Addr::LOCALHOST))),
        };
        let front = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let address = front.local_addr().unwrap();
        let task = tokio::spawn(serve(vec![front], vm_port, target));

        let mut client = TcpStream::connect(address).await.unwrap();
        client.write_all(b"ping").await.unwrap();
        let mut reply = [0u8; 4];
        client.read_exact(&mut reply).await.unwrap();
        assert_eq!(&reply, b"PING");
        task.abort();
    }
}
