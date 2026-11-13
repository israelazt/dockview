//! Keeps the lists of capture cards and audio inputs up to date while the
//! program runs, so a card plugged in later shows up without restarting.
//!
//! Listing devices takes a while and wants a Media Foundation thread, so a
//! thread of its own does it every couple of seconds and the program just reads
//! the last list.

use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use dockview_domain::device::{AudioDevice, DeviceLists, VideoDevice};

use crate::audio::enumerate_audio_devices;
use crate::mf::{enumerate_video_devices, MfSession};
use crate::worker::Worker;

/// How often the devices are listed again.
const SCAN_INTERVAL: Duration = Duration::from_secs(2);
/// How long [`DeviceWatch::start`] waits for the first list.
const FIRST_SCAN_TIMEOUT: Duration = Duration::from_secs(5);
/// How long `Drop` waits for the thread.
const STOP_TIMEOUT: Duration = Duration::from_secs(2);
/// How often the thread checks whether it has to stop while it waits.
const NAP: Duration = Duration::from_millis(100);

/// The newest lists, kept by a background thread. Dropping it stops the thread.
pub struct DeviceWatch {
    lists: Arc<Mutex<DeviceLists>>,
    _worker: Worker,
}

impl DeviceWatch {
    /// Lists the devices once (waiting for it) and keeps listing in the
    /// background.
    pub fn start() -> Self {
        let lists = Arc::new(Mutex::new(DeviceLists::default()));
        let (first_done, first) = mpsc::channel();
        let mut worker = Worker::new(STOP_TIMEOUT);
        let shared = Arc::clone(&lists);
        worker.spawn("device-watch", move |stop| {
            let session = MfSession::start().ok();
            let mut first_done = Some(first_done);
            while !stop.load(Ordering::Relaxed) {
                let video = session
                    .as_ref()
                    .and_then(|s| enumerate_video_devices(s).ok());
                let audio = enumerate_audio_devices().ok();
                {
                    let mut current = shared.lock().unwrap_or_else(|e| e.into_inner());
                    *current = merge(&current, video, audio);
                }
                if let Some(done) = first_done.take() {
                    let _ = done.send(());
                }
                let mut waited = Duration::ZERO;
                while waited < SCAN_INTERVAL && !stop.load(Ordering::Relaxed) {
                    thread::sleep(NAP);
                    waited += NAP;
                }
            }
        });
        let _ = first.recv_timeout(FIRST_SCAN_TIMEOUT);
        Self {
            lists,
            _worker: worker,
        }
    }

    /// The lists as of the last scan.
    pub fn lists(&self) -> DeviceLists {
        self.lists.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

/// The new lists: what a scan found, and for a list the scan could not read,
/// the previous one (a failed scan must not look like every device vanishing).
fn merge(
    previous: &DeviceLists,
    video: Option<Vec<VideoDevice>>,
    audio: Option<Vec<AudioDevice>>,
) -> DeviceLists {
    DeviceLists {
        video: video.unwrap_or_else(|| previous.video.clone()),
        audio: audio.unwrap_or_else(|| previous.audio.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn video(name: &str) -> VideoDevice {
        VideoDevice {
            friendly_name: name.into(),
            symbolic_link: format!("link:{name}"),
        }
    }

    #[test]
    fn a_scan_replaces_the_lists() {
        let previous = DeviceLists {
            video: vec![video("old")],
            audio: Vec::new(),
        };
        let merged = merge(&previous, Some(vec![video("new")]), Some(Vec::new()));
        assert_eq!(merged.video, [video("new")]);
    }

    #[test]
    fn a_list_that_could_not_be_read_keeps_the_previous_one() {
        let previous = DeviceLists {
            video: vec![video("card")],
            audio: vec![AudioDevice {
                friendly_name: "mic".into(),
                device_id: "id".into(),
            }],
        };
        let merged = merge(&previous, None, None);
        assert_eq!(merged, previous);
    }

    #[test]
    fn an_empty_scan_is_a_real_answer_not_a_failure() {
        let previous = DeviceLists {
            video: vec![video("card")],
            audio: Vec::new(),
        };
        assert!(merge(&previous, Some(Vec::new()), Some(Vec::new()))
            .video
            .is_empty());
    }
}
