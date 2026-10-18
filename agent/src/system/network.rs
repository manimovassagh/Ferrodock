//! The network: loopback, and the host's NAT network over DHCP.

use super::run;

pub fn up() -> Result<(), String> {
    run("ip", &["link", "set", "lo", "up"])?;
    run("ip", &["link", "set", "eth0", "up"])?;
    // -q: quit once there's a lease; -n: fail if there's none; -t/-T: try
    // for about ten seconds. The default script sets the address, the
    // route and /etc/resolv.conf (a link into /run).
    run("udhcpc", &["-i", "eth0", "-q", "-n", "-t", "10", "-T", "1"])
}
