//! The cloud emulator, as the app knows it: what it's been asked to do,
//! what it said it has, and why it last failed. Whether it runs is read
//! from the containers.

use crate::docker::ContainerInfo;
use crate::docker::cloud::{CloudInfo, CloudLabels, CloudOp, CloudSpec};
use crate::worker::cloud::CloudEvent;

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
        CloudOp::Info(_) => "ask",
    }
}

impl Cloud {
    /// Notes that `op` was asked for. False if it shouldn't be sent: one
    /// thing at a time.
    pub fn begin(&mut self, op: CloudOp) -> bool {
        if let CloudOp::Info(_) = op {
            return !std::mem::replace(&mut self.reading, true);
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

    /// Takes in what the worker reports. Returns what to tell the user,
    /// and whether it's an error.
    pub fn apply(&mut self, event: CloudEvent) -> Option<(String, bool)> {
        match event {
            CloudEvent::Step(step) => self.step = step,
            CloudEvent::Info(info) => {
                self.reading = false;
                self.info = Some(info);
            }
            CloudEvent::Done(op) => {
                self.working = None;
                // What it has is asked again: it may be another version,
                // or gone.
                self.info = None;
                return Some((
                    match op {
                        CloudOp::Start(_) => "Started the cloud emulator",
                        CloudOp::Stop => "Stopped the cloud emulator",
                        CloudOp::Remove => "Removed the cloud emulator and its data",
                        CloudOp::Reset(_) => "Emptied the cloud emulator",
                        CloudOp::Info(_) => return None,
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
