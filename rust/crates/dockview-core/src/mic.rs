//! Microphone capture for the replay buffer.
//!
//! Unlike [`crate::audio::AudioEngine`] this only listens: the microphone is
//! never played back (that would feed back into the speakers). Every block goes
//! to an [`AudioTap`] for the recorder, and its loudness is published so the
//! interface can show that the microphone is live.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use wasapi::{initialize_mta, DeviceEnumerator, Direction, WasapiError};

use crate::audio::{init_stream, AudioStatus};
use crate::audio_sync::BYTES_PER_FRAME;
use crate::pcm::AudioTap;

const EVENT_TIMEOUT_MS: u32 = 100;
const STOP_TIMEOUT: Duration = Duration::from_secs(2);

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

/// Listens to a microphone while it is alive. Dropping it releases the device.
pub struct MicCapture {
    stop: Arc<AtomicBool>,
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl MicCapture {
    /// Starts listening to `device_id`, or to the Windows default microphone
    /// when `None`. Blocks go to `tap`.
    pub fn start(device_id: Option<String>, gain: f32, tap: Arc<AudioTap>) -> Self {
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
                .name("mic-capture".into())
                .spawn(move || {
                    if let Err(e) = capture_loop(&shared, &stop, device_id.as_deref()) {
                        if !stop.load(Ordering::Relaxed) {
                            shared.set_status(AudioStatus::Disconnected(e.to_string()));
                        }
                    }
                })
                .ok()
        };
        if thread.is_none() {
            shared.set_status(AudioStatus::Failed("could not start the microphone".into()));
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

impl Drop for MicCapture {
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

/// Microphone gain is limited to a sensible range (0 = muted, up to 4x).
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

fn capture_loop(
    shared: &Shared,
    stop: &AtomicBool,
    device_id: Option<&str>,
) -> Result<(), WasapiError> {
    let _ = initialize_mta();
    let enumerator = DeviceEnumerator::new()?;
    let device = match device_id {
        Some(id) => enumerator.get_device(id)?,
        None => enumerator.get_default_device(Direction::Capture)?,
    };
    let mut client = device.get_iaudioclient()?;
    init_stream(&mut client, Direction::Capture, false)?;
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
}
