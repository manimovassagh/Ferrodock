//! Local CI runs of this session: their jobs, steps and logs as they
//! arrive, newest run first.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::docker::ci::{CiEvent, LogLine, RunRequest, State};

/// Runs kept; older ones are dropped, and their artifacts removed.
pub const MAX_RUNS: usize = 20;

/// Log lines kept per step; the oldest go first.
const MAX_LINES: usize = 20_000;

#[derive(Default)]
pub struct CiRuns {
    /// Newest first.
    pub runs: Vec<Run>,
    /// The ID the next run gets; unique across sessions, as the run's
    /// containers and volumes are named after it.
    next: u64,
    /// How many runs this session made, for their numbers.
    count: usize,
}

pub struct Run {
    pub id: u64,
    /// 1 for the session's first run, and so on.
    pub number: usize,
    /// What was asked, to run it again.
    pub request: RunRequest,
    pub state: State,
    pub started: Instant,
    pub ended: Option<Instant>,
    pub jobs: Vec<JobRun>,
    /// Names of the artifacts its jobs uploaded.
    pub artifacts: Vec<String>,
}

#[derive(Default)]
pub struct JobRun {
    pub name: String,
    pub state: State,
    pub started: Option<Instant>,
    pub ended: Option<Instant>,
    pub steps: Vec<StepRun>,
}

#[derive(Default)]
pub struct StepRun {
    pub name: String,
    pub state: State,
    pub started: Option<Instant>,
    pub ended: Option<Instant>,
    pub lines: VecDeque<LogLine>,
    /// Lines dropped to keep the log within [`MAX_LINES`].
    pub dropped: usize,
}

/// How long something took, or has been going.
pub fn elapsed(started: Option<Instant>, ended: Option<Instant>) -> Option<Duration> {
    let started = started?;
    Some(
        ended
            .unwrap_or_else(Instant::now)
            .saturating_duration_since(started),
    )
}

impl Run {
    pub fn duration(&self) -> Duration {
        elapsed(Some(self.started), self.ended).unwrap_or_default()
    }

    /// The workflow's name.
    pub fn title(&self) -> &str {
        &self.request.workflow.name
    }
}

impl CiRuns {
    /// Records a run asked for, giving it its ID. Returns runs dropped from
    /// the list, whose artifacts can go.
    pub fn started(&mut self, request: &mut RunRequest) -> Vec<u64> {
        if self.next == 0 {
            self.next = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(1, |d| d.as_millis() as u64);
        }
        request.run = self.next;
        self.next += 1;
        self.count += 1;
        self.runs.insert(
            0,
            Run {
                id: request.run,
                number: self.count,
                request: request.clone(),
                state: State::Queued,
                started: Instant::now(),
                ended: None,
                jobs: Vec::new(),
                artifacts: Vec::new(),
            },
        );
        // Running ones stay, however many there are.
        let mut dropped = Vec::new();
        while self.runs.len() > MAX_RUNS {
            match self.runs.iter().rposition(|r| r.state.done()) {
                Some(i) => dropped.push(self.runs.remove(i).id),
                None => break,
            }
        }
        dropped
    }

    pub fn get(&self, id: u64) -> Option<&Run> {
        self.runs.iter().find(|r| r.id == id)
    }

    pub fn running(&self) -> usize {
        self.runs.iter().filter(|r| !r.state.done()).count()
    }

    /// Applies an event. Returns what to tell the user, and whether it's
    /// bad news, once a run ends.
    pub fn apply(&mut self, event: CiEvent) -> Option<(String, bool)> {
        let id = match &event {
            CiEvent::Job { run, .. }
            | CiEvent::JobState { run, .. }
            | CiEvent::Step { run, .. }
            | CiEvent::StepState { run, .. }
            | CiEvent::Log { run, .. }
            | CiEvent::Artifact { run, .. }
            | CiEvent::Done { run, .. } => *run,
        };
        let run = self.runs.iter_mut().find(|r| r.id == id)?;
        if run.state == State::Queued {
            run.state = State::Running;
        }
        match event {
            CiEvent::Job { job, name, .. } => run.job(job).name = name,
            CiEvent::JobState { job, state, .. } => {
                let j = run.job(job);
                j.state = state;
                stamp(&mut j.started, &mut j.ended, state);
            }
            CiEvent::Step {
                job, step, name, ..
            } => run.job(job).step(step).name = name,
            CiEvent::StepState {
                job, step, state, ..
            } => {
                let s = run.job(job).step(step);
                s.state = state;
                stamp(&mut s.started, &mut s.ended, state);
            }
            CiEvent::Log {
                job, step, lines, ..
            } => {
                let s = run.job(job).step(step);
                s.lines.extend(lines);
                while s.lines.len() > MAX_LINES {
                    s.lines.pop_front();
                    s.dropped += 1;
                }
            }
            CiEvent::Artifact { name, .. } => {
                if !run.artifacts.contains(&name) {
                    run.artifacts.push(name);
                }
            }
            CiEvent::Done { state, .. } => {
                run.state = state;
                run.ended = Some(Instant::now());
                // Whatever was still going when the run ended stopped with it.
                for job in &mut run.jobs {
                    if !job.state.done() {
                        job.state = State::Cancelled;
                        job.ended = run.ended;
                    }
                    for step in &mut job.steps {
                        if !step.state.done() {
                            step.state = if step.state == State::Queued {
                                State::Skipped
                            } else {
                                State::Cancelled
                            };
                            step.ended = run.ended;
                        }
                    }
                }
                let what = match state {
                    State::Success => "succeeded",
                    State::Failure => "failed",
                    State::Cancelled => "was cancelled",
                    _ => "ended",
                };
                let text = format!("Workflow “{}” {what} locally", run.title());
                return Some((text, state == State::Failure));
            }
        }
        None
    }
}

impl Run {
    fn job(&mut self, index: usize) -> &mut JobRun {
        if index >= self.jobs.len() {
            self.jobs.resize_with(index + 1, JobRun::default);
        }
        &mut self.jobs[index]
    }
}

impl JobRun {
    fn step(&mut self, index: usize) -> &mut StepRun {
        if index >= self.steps.len() {
            self.steps.resize_with(index + 1, StepRun::default);
        }
        &mut self.steps[index]
    }
}

/// Notes when something started and ended, from its new state.
fn stamp(started: &mut Option<Instant>, ended: &mut Option<Instant>, state: State) {
    if state != State::Queued && started.is_none() {
        *started = Some(Instant::now());
    }
    if state.done() && ended.is_none() {
        *ended = Some(Instant::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docker::ci::{LineKind, Workflow};

    fn request() -> RunRequest {
        RunRequest {
            run: 0,
            project: "/code/app".into(),
            workflow: Workflow {
                name: "CI".into(),
                ..Default::default()
            },
            event: "push".into(),
            inputs: Vec::new(),
            secrets: Vec::new(),
        }
    }

    #[test]
    fn follows_a_run_and_drops_old_ones() {
        let mut ci = CiRuns::default();
        let mut req = request();
        assert!(ci.started(&mut req).is_empty());
        let id = req.run;
        assert!(id > 0);
        assert_eq!(ci.running(), 1);

        ci.apply(CiEvent::Job {
            run: id,
            job: 1,
            name: "test".into(),
        });
        ci.apply(CiEvent::Step {
            run: id,
            job: 1,
            step: 0,
            name: "Set up job".into(),
        });
        ci.apply(CiEvent::StepState {
            run: id,
            job: 1,
            step: 0,
            state: State::Running,
        });
        ci.apply(CiEvent::Log {
            run: id,
            job: 1,
            step: 0,
            lines: vec![LogLine {
                text: "hi".into(),
                kind: LineKind::Output,
            }],
        });
        ci.apply(CiEvent::Artifact {
            run: id,
            name: "dist".into(),
        });
        let run = ci.get(id).unwrap();
        assert_eq!(run.state, State::Running);
        assert_eq!(run.jobs.len(), 2);
        assert_eq!(run.jobs[1].steps[0].lines.len(), 1);
        assert_eq!(run.artifacts, ["dist"]);

        let (text, bad) = ci
            .apply(CiEvent::Done {
                run: id,
                state: State::Failure,
            })
            .unwrap();
        assert!(bad && text.contains("“CI” failed"));
        let run = ci.get(id).unwrap();
        assert_eq!(run.jobs[1].steps[0].state, State::Cancelled);
        assert_eq!(ci.running(), 0);

        let mut dropped = Vec::new();
        for _ in 0..MAX_RUNS {
            dropped.extend(ci.started(&mut request()));
        }
        assert_eq!(dropped, [id]);
        assert_eq!(ci.runs.len(), MAX_RUNS);
        assert_eq!(ci.runs[0].number, MAX_RUNS + 1);
    }
}
