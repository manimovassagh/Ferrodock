//! Starting the Docker engine for the user: once when Ferrodock opens and
//! finds it down (if the setting allows), and from the Start button on the
//! "Docker isn't running" screen. Also remembers which app serves the
//! engine while it runs, to start the same one next time.
//!
//! When there's nothing else to start, or it's picked, that's Ferrodock's
//! own engine ([`crate::vm`]): downloaded on first use, then booted.

use std::time::{Duration, Instant};

use super::settings::Settings;
use super::store::{Connection, Store};
use crate::docker::{Engine, Starter, builtin_engine, starter};
use crate::vm;
use crate::worker::{Request, Worker};

/// How Ferrodock's own engine is called on buttons.
const BUILTIN: &str = "Ferrodock's engine";

/// How long a start may take before Ferrodock says it didn't work. Engines
/// that run in a VM can take a minute or two to boot it.
const PATIENCE: Duration = Duration::from_secs(180);

/// What the Start button shows.
#[derive(Debug, Clone, PartialEq)]
pub enum Offer {
    /// Nothing on this machine Ferrodock knows how to start.
    Nothing,
    /// Can start this (e.g. "the Docker engine").
    Start(String),
    /// Can download and start Ferrodock's own engine.
    SetUp,
    /// Downloading Ferrodock's engine: how far, 0 to 1, when known.
    Downloading(Option<f32>),
    /// Starting it now.
    Starting(String),
    /// The last start failed.
    Failed { name: String, error: String },
}

/// How the engine in use gets started.
enum Plan {
    /// Its own app or service.
    App(Starter),
    /// Ferrodock's engine.
    BuiltIn,
}

pub struct EngineStarter {
    /// Ferrodock's own engine.
    pub builtin: vm::Engine,
    /// Wakes the UI while the engine downloads or boots.
    repaint: Option<eframe::egui::Context>,
    /// When the engine was asked to start, and what.
    started: Option<(Instant, String)>,
    error: Option<String>,
    /// Whether starting at launch was tried already.
    tried_at_launch: bool,
    /// Whether the app serving the connected engine was looked up.
    provider_checked: bool,
}

/// The engine in use: its name and host.
fn engine(store: &Store) -> Option<(&str, &str)> {
    let name = store.engine.as_deref()?;
    store
        .engines
        .iter()
        .find(|e| e.name == name)
        .map(|e| (e.name.as_str(), e.host.as_str()))
}

fn host(store: &Store) -> Option<&str> {
    engine(store).map(|(_, host)| host)
}

impl EngineStarter {
    pub fn new(ctx: &eframe::egui::Context) -> Self {
        Self {
            builtin: vm::Engine::new(),
            repaint: Some(ctx.clone()),
            started: None,
            error: None,
            tried_at_launch: false,
            provider_checked: false,
        }
    }

    fn starter(store: &Store, settings: &Settings) -> Option<Starter> {
        let (name, host) = engine(store)?;
        if name == vm::NAME {
            return None;
        }
        starter(host, name, settings.engine_app.as_deref())
    }

    fn plan(store: &Store, settings: &Settings) -> Option<Plan> {
        let builtin_picked = engine(store).is_some_and(|(name, _)| name == vm::NAME);
        if !builtin_picked && let Some(starter) = Self::starter(store, settings) {
            return Some(Plan::App(starter));
        }
        vm::SUPPORTED.then_some(Plan::BuiltIn)
    }

    /// Starts the engine: asks the worker to start its app, or boots
    /// Ferrodock's own. Returns the engine to switch to, when that's
    /// Ferrodock's and another one is in use.
    pub fn start(&mut self, store: &Store, settings: &Settings, worker: &Worker) -> Option<Engine> {
        match Self::plan(store, settings)? {
            Plan::App(starter) => {
                self.started = Some((Instant::now(), starter.name()));
                self.error = None;
                worker.send(Request::StartEngine(starter));
                None
            }
            Plan::BuiltIn => {
                let ctx = self.repaint.clone();
                self.builtin.start(settings.resources(), move || {
                    if let Some(ctx) = &ctx {
                        ctx.request_repaint();
                    }
                });
                let in_use = engine(store).is_some_and(|(name, _)| name == vm::NAME);
                if in_use { None } else { builtin_engine() }
            }
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
    pub fn update(
        &mut self,
        store: &Store,
        settings: &Settings,
        worker: &Worker,
    ) -> Option<Engine> {
        let mut switch = None;
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
                        switch = self.start(store, settings, worker);
                    }
                }
                if let Some((since, name)) = &self.started
                    && since.elapsed() > PATIENCE
                {
                    self.error = Some(format!("{name} didn't answer after 3 minutes"));
                    self.started = None;
                }
                self.builtin.check();
            }
            Connection::Connecting => {}
        }
        switch
    }

    /// Another engine was picked: look up its app once it's connected.
    pub fn engine_switched(&mut self) {
        self.provider_checked = false;
        self.started = None;
        self.error = None;
    }

    pub fn offer(&self, store: &Store, settings: &Settings) -> Offer {
        if let Some(Plan::BuiltIn) = Self::plan(store, settings) {
            return match self.builtin.state() {
                vm::State::NotInstalled => Offer::SetUp,
                vm::State::Downloading { done, total } => {
                    Offer::Downloading(total.filter(|t| *t > 0).map(|t| done as f32 / t as f32))
                }
                // Running but not answering yet: the engine is still
                // coming up inside.
                vm::State::Starting | vm::State::Running => Offer::Starting(BUILTIN.into()),
                vm::State::Stopped => Offer::Start(BUILTIN.into()),
                vm::State::Failed(error) => Offer::Failed {
                    name: BUILTIN.into(),
                    error,
                },
            };
        }
        self.app_offer(Self::starter(store, settings))
    }

    /// What to offer when the engine's own app or service starts it.
    fn app_offer(&self, starter: Option<Starter>) -> Offer {
        if let Some((_, name)) = &self.started {
            return Offer::Starting(name.clone());
        }
        if let Some(error) = &self.error {
            return Offer::Failed {
                name: starter.map_or_else(|| "Docker".into(), |s| s.name()),
                error: error.clone(),
            };
        }
        match starter {
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
            builtin: vm::Engine::new(),
            repaint: None,
            started: Some((Instant::now(), "the Docker engine".into())),
            error: None,
            tried_at_launch: false,
            provider_checked: false,
        };
        assert_eq!(
            starter.app_offer(None),
            Offer::Starting("the Docker engine".into())
        );
        starter.finished(Err("no VM".into()));
        assert!(matches!(
            starter.app_offer(None),
            Offer::Failed { error, .. } if error == "no VM"
        ));
        starter.error = None;
        assert_eq!(
            starter.app_offer(Some(Starter::Service)),
            Offer::Start("the Docker service".into())
        );
    }

    /// With no other engine to start, Ferrodock offers its own where it
    /// has one.
    #[test]
    fn offers_its_own_engine_when_there_is_nothing_else() {
        let starter = EngineStarter {
            builtin: vm::Engine::new(),
            repaint: None,
            started: None,
            error: None,
            tried_at_launch: false,
            provider_checked: false,
        };
        let offer = starter.offer(&Store::new(), &Settings::default());
        if vm::SUPPORTED {
            assert!(matches!(offer, Offer::SetUp | Offer::Start(_)), "{offer:?}");
        } else {
            assert_eq!(offer, Offer::Nothing);
        }
    }
}
