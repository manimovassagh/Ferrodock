//! The cloud emulator, as the app knows it: what it's been asked to do,
//! what it said it has, and why it last failed. Whether it runs is read
//! from the containers.

use crate::docker::ContainerInfo;
use std::collections::VecDeque;

use crate::docker::cloud::{CloudDetail, CloudInfo, CloudLabels, CloudOp, CloudRequest, CloudSpec};
use crate::worker::cloud::CloudEvent;

/// Requests kept for the log; the oldest go first.
pub const MAX_REQUESTS: usize = 2_000;

#[derive(Default)]
pub struct Cloud {
    /// How to run it, from Settings.
    pub spec: CloudSpec,
    /// A start, stop, reset or removal in flight.
    pub working: Option<CloudOp>,
    /// What starting it is doing.
    pub step: String,
    /// What the running emulator said it has, once asked.
    pub info: Option<Result<CloudInfo, String>>,
    /// It's being asked.
    pub reading: bool,
    /// Why it didn't start, until it's tried again.
    pub error: Option<String>,
    /// The requests it answered, oldest first, as far as they've been
    /// read: only while the log is open.
    pub requests: VecDeque<CloudRequest>,
    /// The newest requests are being read.
    pub polling: bool,
    /// Why the requests couldn't be read last time.
    pub requests_error: Option<String>,
    /// The request whose headers and bodies were asked for, and those
    /// once they're here.
    pub detail: Option<(u64, Option<Result<CloudDetail, String>>)>,
}

/// The emulator's container and what it was made with, if there is one.
pub fn emulator(containers: &[ContainerInfo]) -> Option<(&ContainerInfo, CloudLabels)> {
    containers.iter().find_map(|c| Some((c, c.cloud?)))
}

/// "start", "stop"… for messages.
fn verb(op: CloudOp) -> &'static str {
    match op {
        CloudOp::Start(_) => "start",
        CloudOp::Stop => "stop",
        CloudOp::Remove => "remove",
        CloudOp::Reset(_) => "empty",
        CloudOp::Info(_) | CloudOp::Requests { .. } | CloudOp::Detail { .. } => "ask",
    }
}

impl Cloud {
    /// Notes that `op` was asked for. False if it shouldn't be sent: one
    /// thing at a time.
    pub fn begin(&mut self, op: CloudOp) -> bool {
        match op {
            CloudOp::Info(_) => return !std::mem::replace(&mut self.reading, true),
            CloudOp::Requests { .. } => return !std::mem::replace(&mut self.polling, true),
            CloudOp::Detail { id, .. } => {
                if self.detail.as_ref().is_some_and(|(asked, _)| *asked == id) {
                    return false;
                }
                self.detail = Some((id, None));
                return true;
            }
            _ => {}
        }
        if self.working.is_some() {
            return false;
        }
        self.working = Some(op);
        self.step.clear();
        if let CloudOp::Start(_) = op {
            self.error = None;
        }
        true
    }

    /// The ID of the newest request read; 0 before any.
    pub fn last_request(&self) -> u64 {
        self.requests.back().map_or(0, |r| r.id)
    }

    /// Takes in what the worker reports. Returns what to tell the user,
    /// and whether it's an error.
    pub fn apply(&mut self, event: CloudEvent) -> Option<(String, bool)> {
        match event {
            CloudEvent::Step(step) => self.step = step,
            CloudEvent::Info(info) => {
                self.reading = false;
                self.info = Some(info);
            }
            CloudEvent::Requests(read) => {
                self.polling = false;
                match read {
                    Ok((new, last)) => {
                        self.requests_error = None;
                        // IDs behind ours: the emulator started again,
                        // and what we hold is from its last run.
                        if last < self.last_request() {
                            self.requests.clear();
                            self.detail = None;
                        }
                        let known = self.last_request();
                        self.requests
                            .extend(new.into_iter().filter(|r| r.id > known));
                        while self.requests.len() > MAX_REQUESTS {
                            self.requests.pop_front();
                        }
                    }
                    Err(e) => self.requests_error = Some(e),
                }
            }
            CloudEvent::Detail(id, read) => {
                if let Some((asked, detail)) = &mut self.detail
                    && *asked == id
                {
                    *detail = Some(read);
                }
            }
            CloudEvent::Done(op) => {
                self.working = None;
                // Emptied, stopped or started afresh: its log went with it.
                self.requests.clear();
                self.detail = None;
                // What it has is asked again: it may be another version,
                // or gone.
                self.info = None;
                return Some((
                    match op {
                        CloudOp::Start(_) => "Started the cloud emulator",
                        CloudOp::Stop => "Stopped the cloud emulator",
                        CloudOp::Remove => "Removed the cloud emulator and its data",
                        CloudOp::Reset(_) => "Emptied the cloud emulator",
                        CloudOp::Info(_) | CloudOp::Requests { .. } | CloudOp::Detail { .. } => {
                            return None;
                        }
                    }
                    .to_owned(),
                    false,
                ));
            }
            CloudEvent::Failed(op, message) => {
                self.working = None;
                if let CloudOp::Start(_) = op {
                    self.error = Some(message.clone());
                }
                return Some((
                    format!("Couldn't {} the cloud emulator: {message}", verb(op)),
                    true,
                ));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn does_one_thing_at_a_time() {
        let mut cloud = Cloud::default();
        let start = CloudOp::Start(CloudSpec::default());
        assert!(cloud.begin(start));
        assert!(!cloud.begin(CloudOp::Stop));
        assert_eq!(cloud.apply(CloudEvent::Step("Starting…".into())), None);
        assert_eq!(cloud.step, "Starting…");
        // Asking what it has doesn't wait for the rest, but isn't doubled.
        assert!(cloud.begin(CloudOp::Info(4566)));
        assert!(!cloud.begin(CloudOp::Info(4566)));
        cloud.apply(CloudEvent::Info(Ok(CloudInfo::default())));
        assert!(!cloud.reading && cloud.info.is_some());

        let (text, is_error) = cloud.apply(CloudEvent::Done(start)).unwrap();
        assert_eq!(
            (text.as_str(), is_error),
            ("Started the cloud emulator", false)
        );
        assert!(cloud.working.is_none() && cloud.info.is_none());
        assert!(cloud.begin(CloudOp::Stop));
    }

    #[test]
    fn keeps_why_a_start_failed_until_the_next() {
        let mut cloud = Cloud::default();
        let start = CloudOp::Start(CloudSpec::default());
        cloud.begin(start);
        let (text, is_error) = cloud
            .apply(CloudEvent::Failed(start, "Port 4566 is taken.".into()))
            .unwrap();
        assert!(is_error);
        assert_eq!(
            text,
            "Couldn't start the cloud emulator: Port 4566 is taken."
        );
        assert_eq!(cloud.error.as_deref(), Some("Port 4566 is taken."));
        // Other failures are told, not kept.
        cloud.begin(CloudOp::Reset(4566));
        cloud.apply(CloudEvent::Failed(CloudOp::Reset(4566), "no answer".into()));
        assert_eq!(cloud.error.as_deref(), Some("Port 4566 is taken."));
        cloud.begin(start);
        assert_eq!(cloud.error, None);
    }

    #[test]
    fn follows_the_request_log() {
        let request = |id: u64| CloudRequest {
            id,
            operation: format!("Op{id}"),
            ..Default::default()
        };
        let poll = CloudOp::Requests {
            port: 4566,
            after: 0,
        };
        let mut cloud = Cloud::default();
        assert!(cloud.begin(poll));
        assert!(!cloud.begin(poll));
        cloud.apply(CloudEvent::Requests(Ok((vec![request(1), request(2)], 2))));
        assert!(!cloud.polling);
        assert_eq!(cloud.last_request(), 2);
        // One read twice isn't listed twice.
        cloud.apply(CloudEvent::Requests(Ok((vec![request(2), request(3)], 3))));
        assert_eq!(
            cloud.requests.iter().map(|r| r.id).collect::<Vec<_>>(),
            [1, 2, 3]
        );

        // A request's details are asked for once, and kept for that one.
        let detail = CloudOp::Detail { port: 4566, id: 2 };
        assert!(cloud.begin(detail));
        assert!(!cloud.begin(detail));
        cloud.apply(CloudEvent::Detail(7, Ok(CloudDetail::default())));
        assert!(matches!(cloud.detail, Some((2, None))));
        cloud.apply(CloudEvent::Detail(2, Ok(CloudDetail::default())));
        assert!(matches!(cloud.detail, Some((2, Some(Ok(_))))));

        // The emulator started again: its count begins anew.
        cloud.apply(CloudEvent::Requests(Ok((vec![request(1)], 1))));
        assert_eq!(cloud.requests.iter().map(|r| r.id).collect::<Vec<_>>(), [1]);
        assert!(cloud.detail.is_none());

        cloud.apply(CloudEvent::Requests(Err("no answer".into())));
        assert_eq!(cloud.requests_error.as_deref(), Some("no answer"));
        assert_eq!(cloud.requests.len(), 1);
    }

    #[test]
    fn finds_the_emulator_among_containers() {
        let labels = CloudLabels {
            port: 4570,
            keep_data: true,
        };
        let containers = [
            ContainerInfo {
                name: "web".into(),
                ..Default::default()
            },
            ContainerInfo {
                name: "ferrodock-cloud".into(),
                cloud: Some(labels),
                ..Default::default()
            },
        ];
        let (container, found) = emulator(&containers).unwrap();
        assert_eq!(
            (container.name.as_str(), found),
            ("ferrodock-cloud", labels)
        );
        assert!(emulator(&containers[..1]).is_none());
    }
}
