//! What the host tells the agent, through `ferrodock.*` parameters on the
//! kernel command line.

/// Where the engine's socket is offered to the host.
pub const VSOCK_PORT: u32 = 2375;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// The disk that holds images, containers and volumes.
    pub data_disk: String,
    /// virtiofs shares: (tag, where to mount it). The host shares the
    /// user's home folder at the same path, so bind mounts just work.
    pub shares: Vec<(String, String)>,
    /// Also offer the engine on this TCP port (for testing without vsock).
    pub tcp_port: Option<u16>,
    pub hostname: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            data_disk: "/dev/vdb".into(),
            shares: Vec::new(),
            tcp_port: None,
            hostname: "ferrodock".into(),
        }
    }
}

impl Config {
    /// Reads `ferrodock.data=/dev/vdb ferrodock.share=home:/Users
    /// ferrodock.tcp=2375 ferrodock.hostname=…`; anything else is ignored.
    pub fn parse(cmdline: &str) -> Self {
        let mut config = Self::default();
        for word in cmdline.split_whitespace() {
            let Some((key, value)) = word.split_once('=') else {
                continue;
            };
            match key {
                "ferrodock.data" => config.data_disk = value.to_owned(),
                "ferrodock.share" => {
                    if let Some((tag, path)) = value.split_once(':')
                        && !tag.is_empty()
                        && path.starts_with('/')
                    {
                        config.shares.push((tag.to_owned(), path.to_owned()));
                    }
                }
                "ferrodock.tcp" => config.tcp_port = value.parse().ok(),
                "ferrodock.hostname" => config.hostname = value.to_owned(),
                _ => {}
            }
        }
        config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_its_parameters_and_ignores_the_rest() {
        let config = Config::parse(
            "console=hvc0 root=/dev/vda ro ferrodock.share=home:/Users \
             ferrodock.share=bad ferrodock.tcp=2375 ferrodock.data=/dev/vdc",
        );
        assert_eq!(config.shares, [("home".to_owned(), "/Users".to_owned())]);
        assert_eq!(config.tcp_port, Some(2375));
        assert_eq!(config.data_disk, "/dev/vdc");
        assert_eq!(config.hostname, "ferrodock");
        assert_eq!(Config::parse(""), Config::default());
    }
}
