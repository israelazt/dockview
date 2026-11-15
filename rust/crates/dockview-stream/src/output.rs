//! The service that keeps a stream going: connects, sends what the encoders
//! produce, notices when the connection breaks and gets it back with a growing
//! wait, and keeps the numbers the interface shows.
//!
//! It owns no thread and reads no clock: whoever drives it passes `now`, so the
//! whole behaviour is tested with a fake server and a made-up time.

use std::time::{Duration, Instant};

use dockview_domain::broadcast::{AudioConfig, EncodedAudio, StreamState, StreamStats};
use dockview_domain::encoded::EncodedFrame;

use crate::annexb;
use crate::flv;
use crate::rtmp::amf::Value;
use crate::rtmp::publisher::{PublishError, Publisher};
use crate::transport::{Connector, Target};

/// How long one connection attempt may take.
const PATIENCE: Duration = Duration::from_secs(10);
/// Longest wait between two attempts.
const MAX_BACKOFF: Duration = Duration::from_secs(30);

/// What is being streamed.
#[derive(Debug, Clone)]
pub struct StreamConfig {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub video_kbps: u32,
    /// Absent for a stream without sound.
    pub audio: Option<AudioConfig>,
    /// Failed connection attempts in a row before giving up.
    pub max_attempts: u32,
}

/// The wait before attempt number `attempt` (1 is the first retry): 1 s, 2 s,
/// 4 s… up to 30 s.
pub fn backoff(attempt: u32) -> Duration {
    let secs = 1u64 << attempt.saturating_sub(1).min(5);
    Duration::from_secs(secs).min(MAX_BACKOFF)
}

pub struct StreamOutput {
    connector: Box<dyn Connector>,
    target: Target,
    key: String,
    config: StreamConfig,
    publisher: Option<Publisher>,
    state: StreamState,
    stats: StreamStats,
    last_error: Option<String>,
    /// The parameter sets, kept across reconnections.
    params: Option<(Vec<u8>, Vec<u8>)>,
    /// Timestamp (100 ns) that counts as 0 on the current connection.
    base: Option<i64>,
    video_open: bool,
    audio_open: bool,
    attempts: u32,
    next_try: Option<Instant>,
    /// Bytes sent on connections that already ended.
    closed_bytes: u64,
    window_start: Option<Instant>,
    window_bytes: u64,
}

impl StreamOutput {
    pub fn new(
        connector: Box<dyn Connector>,
        target: Target,
        key: &str,
        config: StreamConfig,
    ) -> Self {
        Self {
            connector,
            target,
            key: key.to_string(),
            config,
            publisher: None,
            state: StreamState::Offline,
            stats: StreamStats::default(),
            last_error: None,
            params: None,
            base: None,
            video_open: false,
            audio_open: false,
            attempts: 0,
            next_try: None,
            closed_bytes: 0,
            window_start: None,
            window_bytes: 0,
        }
    }

    pub fn stats(&self) -> StreamStats {
        StreamStats {
            state: self.state,
            ..self.stats
        }
    }

    /// Why the stream is failing or reconnecting, in the server's or the
    /// system's words.
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// Whether the next picture to send should be a keyframe (a new
    /// connection cannot start in the middle of a group of pictures).
    pub fn wants_keyframe(&self) -> bool {
        self.publisher.is_some() && !self.video_open
    }

    /// Starts the stream (blocks while connecting).
    pub fn start(&mut self, now: Instant) {
        self.stats = StreamStats::default();
        self.attempts = 0;
        self.closed_bytes = 0;
        self.window_start = Some(now);
        self.window_bytes = 0;
        self.last_error = None;
        self.state = StreamState::Connecting;
        self.connect(now);
    }

    /// Ends the stream.
    pub fn stop(&mut self) {
        self.drop_connection();
        self.state = StreamState::Offline;
        self.next_try = None;
        self.stats.bitrate_bps = 0;
    }

    /// Sends one picture. While the connection is down it is dropped, and
    /// counted.
    pub fn push_video(&mut self, frame: &EncodedFrame, now: Instant) {
        self.tick(now);
        if !self.state.is_active() {
            return;
        }
        if self.publisher.is_none() {
            self.stats.dropped_frames += 1;
            return;
        }
        if frame.keyframe {
            if let Some(sets) = annexb::parameter_sets(&frame.data) {
                self.params = Some(sets);
            }
        }
        if !self.video_open && !self.open_video(frame, now) {
            return;
        }
        let Some(payload) = flv::video_payload(frame) else {
            return;
        };
        let ts = flv::millis(frame.timestamp - self.base.unwrap_or(frame.timestamp));
        let sent = self.with_publisher(now, |p| p.send_video(ts, payload));
        if sent {
            self.stats.sent_frames += 1;
        }
    }

    /// Sends one audio unit. It waits for the first picture, which fixes the
    /// time zero both share.
    pub fn push_audio(&mut self, unit: &EncodedAudio, now: Instant) {
        let (Some(config), Some(base)) = (self.config.audio.clone(), self.base) else {
            return;
        };
        if !self.video_open || self.publisher.is_none() {
            return;
        }
        if !self.audio_open {
            let header = flv::audio_sequence_header(&config);
            if !self.with_publisher(now, |p| p.send_audio(0, header)) {
                return;
            }
            self.audio_open = true;
        }
        let ts = flv::millis(unit.timestamp - base);
        let payload = flv::audio_payload(&config, unit);
        self.with_publisher(now, |p| p.send_audio(ts, payload));
    }

    /// Housekeeping: retries a lost connection when its wait is over, answers
    /// the server and updates the bitrate. Call it often (every picture is
    /// fine).
    pub fn tick(&mut self, now: Instant) {
        if self.state == StreamState::Reconnecting && self.next_try.is_some_and(|t| now >= t) {
            self.connect(now);
        }
        if self.publisher.is_some() {
            self.with_publisher(now, |p| p.poll());
        }
        self.update_bitrate(now);
    }

    fn open_video(&mut self, frame: &EncodedFrame, now: Instant) -> bool {
        if !frame.keyframe {
            return false;
        }
        let Some((sps, pps)) = self.params.clone().filter(|(s, _)| s.len() >= 4) else {
            return false;
        };
        let header = flv::video_sequence_header(&sps, &pps);
        if !self.with_publisher(now, |p| p.send_video(0, header)) {
            return false;
        }
        self.base = Some(frame.timestamp);
        self.video_open = true;
        true
    }

    /// Runs `send` on the connection; a failure is handled as a lost
    /// connection. Whether it worked.
    fn with_publisher(
        &mut self,
        now: Instant,
        send: impl FnOnce(&mut Publisher) -> Result<(), PublishError>,
    ) -> bool {
        let Some(p) = self.publisher.as_mut() else {
            return false;
        };
        match send(p) {
            Ok(()) => true,
            Err(e) => {
                self.lost(e, now);
                false
            }
        }
    }

    fn connect(&mut self, now: Instant) {
        let reconnecting = self.attempts > 0;
        let opened = self
            .connector
            .connect(&self.target)
            .map_err(PublishError::Io)
            .and_then(|t| Publisher::connect(t, &self.target, &self.key, PATIENCE));
        match opened {
            Ok(mut p) => {
                let info = [
                    ("width", Value::Number(f64::from(self.config.width))),
                    ("height", Value::Number(f64::from(self.config.height))),
                    ("framerate", Value::Number(f64::from(self.config.fps))),
                    (
                        "videodatarate",
                        Value::Number(f64::from(self.config.video_kbps)),
                    ),
                    ("videocodecid", Value::Number(7.0)),
                ];
                if let Err(e) = p.send_metadata(&info) {
                    return self.lost(e, now);
                }
                self.publisher = Some(p);
                self.state = StreamState::Live;
                self.next_try = None;
                if reconnecting {
                    self.stats.reconnects += 1;
                }
                self.attempts = 0;
                self.last_error = None;
            }
            Err(e) => self.lost(e, now),
        }
    }

    fn lost(&mut self, error: PublishError, now: Instant) {
        self.drop_connection();
        self.last_error = Some(error.to_string());
        self.attempts += 1;
        let refused = matches!(error, PublishError::Refused(_));
        if refused || self.attempts > self.config.max_attempts {
            self.state = StreamState::Failed;
            self.next_try = None;
        } else {
            self.state = StreamState::Reconnecting;
            self.next_try = Some(now + backoff(self.attempts));
        }
    }

    fn drop_connection(&mut self) {
        if let Some(p) = self.publisher.take() {
            self.closed_bytes += p.bytes_sent();
        }
        self.base = None;
        self.video_open = false;
        self.audio_open = false;
    }

    fn update_bitrate(&mut self, now: Instant) {
        let Some(start) = self.window_start else {
            return;
        };
        let elapsed = now.saturating_duration_since(start);
        if elapsed < Duration::from_secs(1) {
            return;
        }
        let total = self.closed_bytes + self.publisher.as_ref().map_or(0, Publisher::bytes_sent);
        let bits = total.saturating_sub(self.window_bytes) * 8;
        self.stats.bitrate_bps = (bits as f64 / elapsed.as_secs_f64()) as u32;
        self.window_start = Some(now);
        self.window_bytes = total;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::testing::{FakeConnector, FakeServer, Script};

    struct Rig {
        out: StreamOutput,
        server: FakeServer,
        down: Arc<Mutex<bool>>,
        t0: Instant,
    }

    fn rig(script: Script, audio: bool) -> Rig {
        let server = FakeServer::new(script);
        let down = Arc::new(Mutex::new(false));
        let connector = FakeConnector {
            server: server.clone(),
            down: down.clone(),
        };
        let config = StreamConfig {
            width: 1920,
            height: 1080,
            fps: 60,
            video_kbps: 6000,
            audio: audio.then(|| AudioConfig {
                sample_rate: 48_000,
                channels: 2,
                specific_config: vec![0x11, 0x90],
            }),
            max_attempts: 3,
        };
        let target = Target::parse("rtmp://live.example/app").unwrap();
        Rig {
            out: StreamOutput::new(Box::new(connector), target, "k", config),
            server,
            down,
            t0: Instant::now(),
        }
    }

    fn picture(ms: i64, keyframe: bool) -> EncodedFrame {
        let mut data = Vec::new();
        if keyframe {
            data.extend([0, 0, 0, 1, 0x67, 0x64, 0x00, 0x28, 0, 0, 0, 1, 0x68, 0xEE]);
        }
        data.extend([0, 0, 0, 1, if keyframe { 0x65 } else { 0x41 }, 1, 2]);
        EncodedFrame {
            data,
            timestamp: ms * 10_000,
            duration: 166_666,
            keyframe,
        }
    }

    #[test]
    fn backoff_doubles_up_to_thirty_seconds() {
        let secs: Vec<u64> = (1..=8).map(|n| backoff(n).as_secs()).collect();
        assert_eq!(secs, [1, 2, 4, 8, 16, 30, 30, 30]);
    }

    #[test]
    fn goes_live_and_sends_headers_before_pictures() {
        let mut r = rig(Script::default(), true);
        r.out.start(r.t0);
        assert_eq!(r.out.stats().state, StreamState::Live);
        r.out.push_video(&picture(5000, true), r.t0);
        r.out.push_video(&picture(5016, false), r.t0);
        r.out.push_audio(
            &EncodedAudio {
                data: vec![7],
                timestamp: 50_000_000,
                duration: 0,
            },
            r.t0,
        );
        let video = r.server.seen(|s| s.video.clone());
        assert_eq!(video.len(), 3); // sequence header + two pictures
        assert_eq!(video[0].payload[1], 0);
        assert_eq!(video[1].timestamp, 0, "time zero is the first keyframe");
        assert_eq!(video[2].timestamp, 16);
        assert_eq!(r.server.seen(|s| s.audio.len()), 2); // sequence header + unit
        assert_eq!(r.out.stats().sent_frames, 2);
    }

    #[test]
    fn pictures_before_a_keyframe_are_not_sent() {
        let mut r = rig(Script::default(), false);
        r.out.start(r.t0);
        assert!(r.out.wants_keyframe());
        r.out.push_video(&picture(0, false), r.t0);
        assert_eq!(r.server.seen(|s| s.video.len()), 0);
        r.out.push_video(&picture(16, true), r.t0);
        assert!(!r.out.wants_keyframe());
        assert_eq!(r.out.stats().sent_frames, 1);
    }

    #[test]
    fn a_broken_connection_reconnects_after_the_wait_and_waits_for_a_keyframe() {
        let mut r = rig(Script::default(), false);
        r.out.start(r.t0);
        r.out.push_video(&picture(0, true), r.t0);
        *r.down.lock().unwrap() = true;
        r.server.set_script(Script {
            cut_after: Some(0),
            ..Script::default()
        });
        r.out.push_video(&picture(16, false), r.t0);
        assert_eq!(r.out.stats().state, StreamState::Reconnecting);
        r.out
            .push_video(&picture(33, false), r.t0 + Duration::from_millis(500));
        assert_eq!(r.out.stats().dropped_frames, 1, "dropped while down");
        // The wait is over and the network is back.
        *r.down.lock().unwrap() = false;
        r.server.set_script(Script::default());
        let later = r.t0 + Duration::from_secs(2);
        r.out.push_video(&picture(50, false), later);
        assert_eq!(r.out.stats().state, StreamState::Live);
        assert_eq!(r.out.stats().reconnects, 1);
        assert!(r.out.wants_keyframe());
        r.out.push_video(&picture(66, true), later);
        assert!(!r.out.wants_keyframe());
    }

    #[test]
    fn gives_up_after_too_many_attempts() {
        let mut r = rig(Script::default(), false);
        *r.down.lock().unwrap() = true;
        r.out.start(r.t0);
        for n in 1..=5 {
            r.out.tick(r.t0 + Duration::from_secs(40 * n));
        }
        assert_eq!(r.out.stats().state, StreamState::Failed);
        assert!(r.out.last_error().is_some());
    }

    #[test]
    fn a_refused_key_fails_at_once_without_retrying() {
        let mut r = rig(
            Script {
                publish_status: Some("NetStream.Publish.BadName".into()),
                ..Script::default()
            },
            false,
        );
        r.out.start(r.t0);
        assert_eq!(r.out.stats().state, StreamState::Failed);
        assert!(r.out.last_error().unwrap().contains("BadName"));
    }

    #[test]
    fn stop_goes_offline_and_ignores_pictures() {
        let mut r = rig(Script::default(), false);
        r.out.start(r.t0);
        r.out.stop();
        r.out.push_video(&picture(0, true), r.t0);
        assert_eq!(r.out.stats().state, StreamState::Offline);
        assert_eq!(r.out.stats().sent_frames, 0);
        assert_eq!(r.out.stats().dropped_frames, 0);
    }

    #[test]
    fn measures_the_bitrate_over_a_second() {
        let mut r = rig(Script::default(), false);
        r.out.start(r.t0);
        r.out.push_video(&picture(0, true), r.t0);
        r.out.tick(r.t0 + Duration::from_secs(1));
        assert!(r.out.stats().bitrate_bps > 0);
    }
}
