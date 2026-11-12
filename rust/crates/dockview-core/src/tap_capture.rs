//! Audio capture that only feeds the recorder.
//!
//! Unlike [`crate::audio::AudioEngine`] this never plays anything back: a
//! microphone played through the speakers would feed back, and the sound of a
//! game or of the whole PC is already playing by itself. Every block goes to an
//! [`AudioTap`], and its loudness is published so the interface can show that
//! the source is live.
//!
//! The source is a microphone, the whole PC's sound (loopback of the default
//! output) or the sound of one program and the programs it starts (process
//! loopback, Windows 10 2004 or later).

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use wasapi::{initialize_mta, AudioClient, DeviceEnumerator, Direction, StreamMode, WasapiError};

use crate::audio::{format, init_stream, AudioStatus};
use crate::audio_sync::BYTES_PER_FRAME;
use crate::pcm::AudioTap;

const EVENT_TIMEOUT_MS: u32 = 100;
const STOP_TIMEOUT: Duration = Duration::from_secs(2);

/// What to listen to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureSource {
    /// A microphone: the given device, or the Windows default when `None`.
    Device(Option<String>),
    /// Everything the PC plays on its default output.
    SystemLoopback,
    /// Only the program with this process id (and the programs it started).
    ProcessLoopback(u32),
}

struct Shared {
    status: Mutex<AudioStatus>,
    /// Gain applied before the recorder sees the audio (`f32` bits).
    gain: AtomicU32,
    /// Loudest sample of the last block, 0.0–1.0 (`f32` bits).
    level: AtomicU32,
    tap: Arc<AudioTap>,
}

impl Shared {
    fn set_status(&self, status: AudioStatus) {
        *self.status.lock().unwrap_or_else(|e| e.into_inner()) = status;
    }
}

/// Listens to a [`CaptureSource`] while it is alive. Dropping it releases the
/// device.
pub struct TapCapture {
    stop: Arc<AtomicBool>,
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl TapCapture {
    /// Starts listening to `source`. Blocks go to `tap`.
    pub fn start(source: CaptureSource, gain: f32, tap: Arc<AudioTap>) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let shared = Arc::new(Shared {
            status: Mutex::new(AudioStatus::Starting),
            gain: AtomicU32::new(clamp_gain(gain).to_bits()),
            level: AtomicU32::new(0.0f32.to_bits()),
            tap,
        });
        let thread = {
            let (shared, stop) = (Arc::clone(&shared), Arc::clone(&stop));
            thread::Builder::new()
                .name("audio-tap-capture".into())
                .spawn(move || {
                    if let Err(e) = capture_loop(&shared, &stop, &source) {
                        if !stop.load(Ordering::Relaxed) {
                            shared.set_status(AudioStatus::Disconnected(e.to_string()));
                        }
                    }
                })
                .ok()
        };
        if thread.is_none() {
            shared.set_status(AudioStatus::Failed(
                "could not start the audio capture".into(),
            ));
        }
        Self {
            stop,
            shared,
            thread,
        }
    }

    pub fn status(&self) -> AudioStatus {
        self.shared
            .status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// How loud the last block was, 0.0–1.0 (after the gain).
    pub fn level(&self) -> f32 {
        f32::from_bits(self.shared.level.load(Ordering::Relaxed))
    }

    pub fn set_gain(&self, gain: f32) {
        self.shared
            .gain
            .store(clamp_gain(gain).to_bits(), Ordering::Relaxed);
    }
}

impl Drop for TapCapture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let deadline = Instant::now() + STOP_TIMEOUT;
            while !thread.is_finished() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(5));
            }
            if thread.is_finished() {
                let _ = thread.join();
            }
        }
    }
}

/// Gain is limited to a sensible range (0 = muted, up to 4x).
pub fn clamp_gain(gain: f32) -> f32 {
    if gain.is_nan() {
        1.0
    } else {
        gain.clamp(0.0, 4.0)
    }
}

/// Applies `gain` in place and returns the loudest sample afterwards (0.0–1.0).
pub fn apply_gain(samples: &mut [f32], gain: f32) -> f32 {
    let mut peak = 0.0f32;
    for sample in samples {
        *sample = (*sample * gain).clamp(-1.0, 1.0);
        peak = peak.max(sample.abs());
    }
    peak
}

/// Opens the audio client for `source`, started and ready to read.
fn open_client(source: &CaptureSource) -> Result<AudioClient, WasapiError> {
    let enumerator = DeviceEnumerator::new()?;
    let client = match source {
        CaptureSource::Device(id) => {
            let device = match id {
                Some(id) => enumerator.get_device(id)?,
                None => enumerator.get_default_device(Direction::Capture)?,
            };
            let mut client = device.get_iaudioclient()?;
            init_stream(&mut client, Direction::Capture, false)?;
            client
        }
        CaptureSource::SystemLoopback => {
            // A loopback stream is a *capture* stream opened on the output device.
            let device = enumerator.get_default_device(Direction::Render)?;
            let mut client = device.get_iaudioclient()?;
            init_stream(&mut client, Direction::Capture, false)?;
            client
        }
        CaptureSource::ProcessLoopback(pid) => {
            let mut client = AudioClient::new_application_loopback_client(*pid, true)?;
            // Process loopback cannot report its period; the buffer size is ignored.
            let mode = StreamMode::EventsShared {
                autoconvert: true,
                buffer_duration_hns: 0,
            };
            client.initialize_client(&format(), Direction::Capture, mode, None)?;
            client
        }
    };
    Ok(client)
}

fn capture_loop(
    shared: &Shared,
    stop: &AtomicBool,
    source: &CaptureSource,
) -> Result<(), WasapiError> {
    let _ = initialize_mta();
    let client = open_client(source)?;
    let event = client.set_get_eventhandle()?;
    let capture = client.get_audiocaptureclient()?;
    client.start_stream()?;
    shared.set_status(AudioStatus::Running);

    let mut bytes: Vec<u8> = Vec::new();
    let mut samples: Vec<f32> = Vec::new();
    while !stop.load(Ordering::Relaxed) {
        let _ = event.wait_for_event(EVENT_TIMEOUT_MS);
        while let Some(frames) = capture.get_next_packet_size()? {
            if frames == 0 {
                break;
            }
            bytes.resize(frames as usize * BYTES_PER_FRAME, 0);
            let (read, _) = capture.read_from_device(&mut bytes)?;
            samples.clear();
            let (words, _) = bytes[..read as usize * BYTES_PER_FRAME].as_chunks::<4>();
            samples.extend(words.iter().map(|b| f32::from_le_bytes(*b)));
            let gain = f32::from_bits(shared.gain.load(Ordering::Relaxed));
            let peak = apply_gain(&mut samples, gain);
            shared.level.store(peak.to_bits(), Ordering::Relaxed);
            shared.tap.offer(Instant::now(), &samples);
        }
    }
    let _ = client.stop_stream();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gain_scales_clips_and_reports_the_peak() {
        let mut samples = [0.1, -0.2, 0.4, -0.4];
        let peak = apply_gain(&mut samples, 2.0);
        assert_eq!(samples, [0.2, -0.4, 0.8, -0.8]);
        assert!((peak - 0.8).abs() < 1e-6);

        let mut loud = [0.6, -0.9];
        assert_eq!(apply_gain(&mut loud, 4.0), 1.0);
        assert_eq!(loud, [1.0, -1.0], "clipped, not wrapped");

        let mut muted = [0.5, -0.5];
        assert_eq!(apply_gain(&mut muted, 0.0), 0.0);
        assert_eq!(muted, [0.0, 0.0]);
    }

    #[test]
    fn gain_is_kept_in_range() {
        assert_eq!(clamp_gain(-1.0), 0.0);
        assert_eq!(clamp_gain(9.0), 4.0);
        assert_eq!(clamp_gain(f32::NAN), 1.0);
        assert_eq!(clamp_gain(1.5), 1.5);
    }

    #[test]
    fn a_source_is_compared_by_what_it_listens_to() {
        assert_eq!(CaptureSource::Device(None), CaptureSource::Device(None));
        assert_ne!(
            CaptureSource::ProcessLoopback(1),
            CaptureSource::ProcessLoopback(2)
        );
        assert_ne!(CaptureSource::SystemLoopback, CaptureSource::Device(None));
    }
}
