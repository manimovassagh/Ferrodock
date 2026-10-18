//! Starting the Docker engine for the user: once when Ferrodock opens and
//! finds it down (if the setting allows), and from the Start button on the
//! "Docker isn't running" screen. Also remembers which app serves the
//! engine while it runs, to start the same one next time.

use std::time::{Duration, Instant};

use super::settings::Settings;
use super::store::{Connection, Store};
use crate::docker::{Starter, starter};
use crate::worker::{Request, Worker};

/// How long a start may take before Ferrodock says it didn't work. Colima
/// and Docker Desktop can take a minute or two to boot their VM.
const PATIENCE: Duration = Duration::from_secs(180);

/// What the Start button shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Offer {
    /// Nothing on this machine Ferrodock knows how to start.
    Nothing,
    /// Can start this (e.g. "Docker Desktop").
    Start(String),
    /// Starting it now.
    Starting(String),
    /// The last start failed.
    Failed { name: String, error: String },
}

#[derive(Default)]
pub struct EngineStarter {
    /// When the engine was asked to start, and what.
    started: Option<(Instant, String)>,
    error: Option<String>,
    /// Whether starting at launch was tried already.
    tried_at_launch: bool,
    /// Whether the app serving the connected engine was looked up.
    provider_checked: bool,
}

/// The host of the engine in use.
fn host(store: &Store) -> Option<&str> {
    let name = store.engine.as_deref()?;
    store
        .engines
        .iter()
        .find(|e| e.name == name)
        .map(|e| e.host.as_str())
}

impl EngineStarter {
    fn starter(store: &Store, settings: &Settings) -> Option<Starter> {
        starter(host(store)?, settings.engine_app.as_deref())
    }

    /// Asks the worker to start the engine.
    pub fn start(&mut self, store: &Store, settings: &Settings, worker: &Worker) {
        if let Some(starter) = Self::starter(store, settings) {
            self.started = Some((Instant::now(), starter.name()));
            self.error = None;
            worker.send(Request::StartEngine(starter));
        }
    }

    /// A start finished (the command ran; the engine may still be booting).
    pub fn finished(&mut self, result: Result<(), String>) {
        if let Err(e) = result {
            self.started = None;
            self.error = Some(e);
        }
    }

    /// Runs once per frame: starts the engine at launch if it's down,
    /// gives up on starts that take too long, and looks up who serves the
    /// engine once connected.
    pub fn update(&mut self, store: &Store, settings: &Settings, worker: &Worker) {
        match &store.connection {
            Connection::Connected { .. } => {
                self.started = None;
                self.error = None;
                // Connected means there's no need to start it at launch.
                self.tried_at_launch = true;
                if !self.provider_checked {
                    self.provider_checked = true;
                    if let Some(host) = host(store) {
                        worker.send(Request::FindProvider(host.to_owned()));
                    }
                }
            }
            Connection::Disconnected { reason } => {
                if !self.tried_at_launch {
                    self.tried_at_launch = true;
                    if settings.start_engine && super::shell::is_not_running(reason) {
                        self.start(store, settings, worker);
                    }
                }
                if let Some((since, name)) = &self.started
                    && since.elapsed() > PATIENCE
                {
                    self.error = Some(format!("{name} didn't answer after 3 minutes"));
                    self.started = None;
                }
            }
            Connection::Connecting => {}
        }
    }

    /// Another engine was picked: look up its app once it's connected.
    pub fn engine_switched(&mut self) {
        self.provider_checked = false;
        self.started = None;
        self.error = None;
    }

    pub fn offer(&self, store: &Store, settings: &Settings) -> Offer {
        let name = || {
            Self::starter(store, settings)
                .map(|s| s.name())
                .unwrap_or_else(|| "Docker".into())
        };
        if let Some((_, name)) = &self.started {
            return Offer::Starting(name.clone());
        }
        if let Some(error) = &self.error {
            return Offer::Failed {
                name: name(),
                error: error.clone(),
            };
        }
        match Self::starter(store, settings) {
            Some(starter) => Offer::Start(starter.name()),
            None => Offer::Nothing,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_start_is_offered_again() {
        let mut starter = EngineStarter {
            started: Some((Instant::now(), "Colima".into())),
            ..Default::default()
        };
        let store = Store::new();
        let settings = Settings::default();
        assert_eq!(
            starter.offer(&store, &settings),
            Offer::Starting("Colima".into())
        );
        starter.finished(Err("no VM".into()));
        assert!(matches!(
            starter.offer(&store, &settings),
            Offer::Failed { error, .. } if error == "no VM"
        ));
    }
}
