//! `docker compose` jobs the UI shows: the last command run (up, rebuild,
//! pull & recreate) and the logs being followed.

use std::collections::VecDeque;

use crate::docker::{ComposeAction, ComposeOp};

/// Lines kept per job.
const MAX_LINES: usize = 5000;

pub struct Job {
    pub id: u64,
    /// What it does, e.g. "Rebuilding web in shop".
    pub title: String,
    /// The project it's about.
    pub project: String,
    pub lines: VecDeque<String>,
    /// Set when it ended: the error, if it failed.
    pub ended: Option<Option<String>>,
}

impl Job {
    fn new(id: u64, title: String, project: String) -> Self {
        Self {
            id,
            title,
            project,
            lines: VecDeque::new(),
            ended: None,
        }
    }
}

#[derive(Default)]
pub struct ComposeJobs {
    pub run: Option<Job>,
    pub logs: Option<Job>,
}

impl ComposeJobs {
    /// Records a request. Returns a job to cancel: the logs it replaces.
    pub fn requested(&mut self, op: &ComposeOp) -> Option<u64> {
        match op {
            ComposeOp::Run {
                job,
                target,
                action,
            } => {
                let project = target.label();
                let title = match action {
                    ComposeAction::Up => format!("Starting {project}"),
                    ComposeAction::Rebuild(Some(service)) => {
                        format!("Rebuilding {service} in {project}")
                    }
                    ComposeAction::Rebuild(None) => format!("Rebuilding {project}"),
                    ComposeAction::PullRecreate => format!("Pulling and recreating {project}"),
                };
                self.run = Some(Job::new(*job, title, project));
                None
            }
            ComposeOp::Logs { job, project } => {
                let old = self.logs.take().filter(|j| j.ended.is_none()).map(|j| j.id);
                self.logs = Some(Job::new(
                    *job,
                    format!("Logs of {project}"),
                    project.clone(),
                ));
                old
            }
            ComposeOp::Cancel { job } => {
                if self.logs.as_ref().is_some_and(|j| j.id == *job) {
                    self.logs = None;
                }
                None
            }
        }
    }

    fn job(&mut self, id: u64) -> Option<&mut Job> {
        [self.run.as_mut(), self.logs.as_mut()]
            .into_iter()
            .flatten()
            .find(|j| j.id == id)
    }

    pub fn output(&mut self, id: u64, lines: Vec<String>) {
        if let Some(job) = self.job(id) {
            job.lines.extend(lines);
            while job.lines.len() > MAX_LINES {
                job.lines.pop_front();
            }
        }
    }

    /// Marks a job ended. Returns what to tell the user when a command
    /// (not the logs) finished.
    pub fn done(&mut self, id: u64, error: Option<String>) -> Option<(String, bool)> {
        let is_run = self.run.as_ref().is_some_and(|j| j.id == id);
        let job = self.job(id)?;
        let notice = is_run.then(|| match &error {
            None => (format!("{}: done", job.title), false),
            Some(e) => (format!("{}: {e}", job.title), true),
        });
        job.ended = Some(error);
        notice
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docker::ComposeTarget;

    #[test]
    fn follows_jobs() {
        let mut jobs = ComposeJobs::default();
        let target = ComposeTarget {
            name: Some("shop".into()),
            files: Vec::new(),
            dir: Default::default(),
        };
        jobs.requested(&ComposeOp::Run {
            job: 1,
            target,
            action: ComposeAction::Rebuild(Some("web".into())),
        });
        jobs.output(1, vec!["building".into()]);
        assert_eq!(jobs.run.as_ref().unwrap().title, "Rebuilding web in shop");
        assert_eq!(
            jobs.done(1, None),
            Some(("Rebuilding web in shop: done".into(), false))
        );
        let logs = |job| ComposeOp::Logs {
            job,
            project: "shop".into(),
        };
        assert_eq!(jobs.requested(&logs(2)), None);
        // New logs replace (and cancel) the old ones.
        assert_eq!(jobs.requested(&logs(3)), Some(2));
        assert_eq!(jobs.done(3, None), None, "logs ending isn't news");
    }
}
