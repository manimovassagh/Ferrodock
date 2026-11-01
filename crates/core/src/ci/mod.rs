//! Local CI: runs a project's GitHub Actions workflows on this engine, the
//! way GitHub would, to see how they'd go before pushing. Written for
//! Ferrodock; no other runner is involved.
//!
//! - [`workflow`] reads `.github/workflows/*.yml`.
//! - [`expr`] evaluates `${{ … }}` expressions.
//! - [`matrix`] expands a job's `strategy.matrix`.
//! - [`commands`] reads what steps print (`::group::`, `::error::`…) and
//!   the `GITHUB_ENV` / `GITHUB_OUTPUT` / `GITHUB_PATH` files.
//! - [`image`] is the runner image jobs run in.
//! - [`actions`] are the actions built in (`actions/checkout`, …).
//! - [`context`] is the `github` context, from the project's `.git`.
//! - [`workspace`] is the project as a job sees it, and `hashFiles`.
//! - [`runner`] runs a workflow: its jobs in containers, step by step.
//! - [`fetch`] downloads actions from GitHub; [`metadata`] reads their
//!   `action.yml`.
//! - `external` runs those actions: JavaScript ones, and composite ones.
//! - [`artifacts`] saves what a run uploaded.
//! - [`secrets`] keeps the secrets of local runs, per project.

pub mod actions;
pub mod artifacts;
pub mod commands;
pub mod context;
pub mod expr;
mod external;
pub mod fetch;
pub mod image;
pub mod matrix;
pub mod metadata;
pub mod runner;
pub mod secrets;
pub mod workflow;
pub mod workspace;

pub use runner::{RunRequest, cancel_run, run_workflow};
pub use workflow::{Workflow, find_workflows};

/// Ferrodock's own folder: `~/.ferrodock` (on Windows, in local app data).
pub fn data_dir() -> Option<std::path::PathBuf> {
    use std::path::PathBuf;
    Some(if cfg!(windows) {
        PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("Ferrodock")
    } else {
        PathBuf::from(std::env::var_os("HOME")?).join(".ferrodock")
    })
}

/// What the app asks of Local CI.
#[derive(Debug, Clone, PartialEq)]
pub enum CiOp {
    Run(Box<RunRequest>),
    Cancel(u64),
    /// Save an artifact of a run to the Downloads folder.
    SaveArtifact {
        run: u64,
        name: String,
    },
    /// Remove what a run kept (its artifacts), once it's out of the list.
    Forget(u64),
}

/// Marks every container, network and volume of a run; the value is the
/// run's number.
pub const CI_LABEL: &str = "dev.ferrodock.ci";

/// Where a job, step or run stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum State {
    #[default]
    Queued,
    Running,
    Success,
    Failure,
    Skipped,
    Cancelled,
}

impl State {
    pub fn done(self) -> bool {
        !matches!(self, State::Queued | State::Running)
    }

    pub fn label(self) -> &'static str {
        match self {
            State::Queued => "Queued",
            State::Running => "Running",
            State::Success => "Succeeded",
            State::Failure => "Failed",
            State::Skipped => "Skipped",
            State::Cancelled => "Cancelled",
        }
    }
}

/// A line of a step's log, and how to show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLine {
    pub text: String,
    pub kind: LineKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Output,
    /// The start of a `::group::`; its lines follow until [`LineKind::EndGroup`].
    Group,
    EndGroup,
    Error,
    Warning,
    Notice,
    /// What Ferrodock itself says, e.g. the command it runs.
    Info,
}

impl LogLine {
    pub fn info(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: LineKind::Info,
        }
    }
}

/// What a run reports as it goes. Jobs and steps are numbered in the order
/// they're announced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CiEvent {
    /// A job (one per matrix combination) is known.
    Job {
        run: u64,
        job: usize,
        name: String,
    },
    JobState {
        run: u64,
        job: usize,
        state: State,
    },
    /// A step of a job is known. "Set up job" and "Complete job" are steps
    /// too, as on GitHub.
    Step {
        run: u64,
        job: usize,
        step: usize,
        name: String,
    },
    StepState {
        run: u64,
        job: usize,
        step: usize,
        state: State,
    },
    Log {
        run: u64,
        job: usize,
        step: usize,
        lines: Vec<LogLine>,
    },
    /// A file a job saved with `actions/upload-artifact`.
    Artifact {
        run: u64,
        name: String,
    },
    Done {
        run: u64,
        state: State,
    },
}
