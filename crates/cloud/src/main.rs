//! ferrocloud: Ferrodock's local cloud emulator, as a program.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::process::ExitCode;

use ferrodock_cloud::DEFAULT_PORT;
use ferrodock_cloud::server::{App, serve};
use ferrodock_cloud::service::Registry;
use ferrodock_cloud::store::Store;

const USAGE: &str = "\
ferrocloud: Ferrodock's local cloud emulator

Usage: ferrocloud [OPTIONS]

Options:
  --port <PORT>   The port to listen on [default: 4566] [env: FERROCLOUD_PORT]
  --host <ADDR>   The address to listen on [default: 0.0.0.0]
  --data <DIR>    Keep data in this folder [env: FERROCLOUD_DATA]
  --ephemeral     Keep data in memory only, even when --data is set
  --check         Ask the ferrocloud on this port whether it's up, and exit
                  0 or 1: a container health check that needs no other tool
  -V, --version   Print the version
  -h, --help      Print this help
";

struct Options {
    port: u16,
    host: IpAddr,
    data: Option<PathBuf>,
    check: bool,
}

fn options() -> Result<Option<Options>, String> {
    let mut port = match std::env::var("FERROCLOUD_PORT") {
        Ok(port) => port
            .parse()
            .map_err(|_| format!("FERROCLOUD_PORT isn't a port: {port}"))?,
        Err(_) => DEFAULT_PORT,
    };
    let mut host = IpAddr::V4(Ipv4Addr::UNSPECIFIED);
    let mut data = std::env::var_os("FERROCLOUD_DATA").map(PathBuf::from);
    let mut ephemeral = false;
    let mut check = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = |name: &str| args.next().ok_or(format!("{name} needs a value"));
        match arg.as_str() {
            "--port" => {
                let v = value("--port")?;
                port = v.parse().map_err(|_| format!("--port isn't a port: {v}"))?;
            }
            "--host" => {
                let v = value("--host")?;
                host = v
                    .parse()
                    .map_err(|_| format!("--host isn't an address: {v}"))?;
            }
            "--data" => data = Some(PathBuf::from(value("--data")?)),
            "--ephemeral" => ephemeral = true,
            "--check" => check = true,
            "-V" | "--version" => {
                println!("ferrocloud {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            "-h" | "--help" => {
                print!("{USAGE}");
                return Ok(None);
            }
            other => return Err(format!("unknown option: {other}\n\n{USAGE}")),
        }
    }
    if ephemeral {
        data = None;
    }
    Ok(Some(Options {
        port,
        host,
        data,
        check,
    }))
}

/// Whether a ferrocloud answers on this computer's `port`.
fn is_up(port: u16) -> bool {
    use std::io::{Read, Write};
    use std::time::Duration;
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let Ok(mut stream) = std::net::TcpStream::connect_timeout(&address, Duration::from_secs(2))
    else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let request =
        "GET /_ferrocloud/health HTTP/1.1\r\nhost: localhost\r\nconnection: close\r\n\r\n";
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }
    let mut answer = String::new();
    let _ = stream.read_to_string(&mut answer);
    answer.starts_with("HTTP/1.1 200") && answer.contains("\"status\":\"running\"")
}

/// Completes when the program is asked to stop: Ctrl-C, or a container
/// being stopped.
async fn stopped() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut terminate = signal(SignalKind::terminate()).expect("a signal handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = terminate.recv() => {}
        }
    }
    #[cfg(not(unix))]
    let _ = tokio::signal::ctrl_c().await;
}

#[tokio::main]
async fn main() -> ExitCode {
    let options = match options() {
        Ok(Some(options)) => options,
        Ok(None) => return ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    if options.check {
        return if is_up(options.port) {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }
    let app = Store::open(options.data.as_deref())
        .and_then(|store| App::new(Registry::standard(), store));
    let app = match app {
        Ok(app) => app,
        Err(e) => {
            eprintln!("ferrocloud: {e}");
            return ExitCode::FAILURE;
        }
    };
    let address = SocketAddr::new(options.host, options.port);
    let listener = match tokio::net::TcpListener::bind(address).await {
        Ok(listener) => listener,
        Err(e) => {
            eprintln!("ferrocloud: can't listen on {address}: {e}");
            return ExitCode::FAILURE;
        }
    };
    match &options.data {
        Some(dir) => println!(
            "ferrocloud listening on {address}, data in {}",
            dir.display()
        ),
        None => println!("ferrocloud listening on {address}, data in memory"),
    }
    serve(listener, app.clone(), stopped()).await;
    // Straight out, without waiting for open connections (the request
    // feed never ends): a container being stopped gets a few seconds.
    app.store.clean_up();
    std::process::exit(0)
}
