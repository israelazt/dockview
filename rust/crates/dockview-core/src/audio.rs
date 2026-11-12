//! WASAPI audio: capture from the capture card's audio endpoint and play it
//! on the default output device, with volume, sync offset and drift control.
//!
//! Pipeline: capture thread → [`AudioRing`] → render thread. Both ends use
//! event-driven shared mode with `f32` / 48 kHz / stereo; WASAPI converts
//! to and from the device's native format.

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use wasapi::{
    initialize_mta, AudioClient, DeviceEnumerator, Direction, SampleType, StreamMode, WasapiError,
    WaveFormat,
};

use crate::audio_sync::{
    plan_correction, target_frames, AudioRing, BYTES_PER_FRAME, CHANNELS, SAMPLE_RATE,
};
use crate::device::AudioDevice;
use crate::pcm::AudioTap;

/// How often the render thread re-evaluates drift.
const DRIFT_CHECK_INTERVAL: Duration = Duration::from_millis(500);
/// Event wait timeout; also how quickly the threads notice a stop request.
const EVENT_TIMEOUT_MS: u32 = 100;
/// How long `Drop` waits for the audio threads before detaching them.
const STOP_TIMEOUT: Duration = Duration::from_secs(2);

/// Lifecycle of an [`AudioEngine`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioStatus {
    Starting,
    Running,
    /// The capture device went away or stopped delivering.
    Disconnected(String),
    Failed(String),
    Stopped,
}

struct Shared {
    ring: Mutex<AudioRing>,
    volume: AtomicU32,
    offset_ms: AtomicI32,
    status: Mutex<AudioStatus>,
    /// Receives a copy of every captured block, for the replay recorder.
    tap: Mutex<Option<Arc<AudioTap>>>,
}

impl Shared {
    fn ring(&self) -> MutexGuard<'_, AudioRing> {
        self.ring.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn set_status(&self, status: AudioStatus) {
        *self.status.lock().unwrap_or_else(|e| e.into_inner()) = status;
    }

    fn volume(&self) -> f32 {
        f32::from_bits(self.volume.load(Ordering::Relaxed))
    }
}

/// Runs the audio pipeline. Dropping it stops both threads.
pub struct AudioEngine {
    stop: Arc<AtomicBool>,
    shared: Arc<Shared>,
    threads: Vec<JoinHandle<()>>,
}

impl AudioEngine {
    /// Starts capturing from `capture_device_id` and playing on the default
    /// output. `volume` is 0.0–1.0; `offset_ms` is the initial sync offset
    /// (positive delays the audio).
    pub fn start(capture_device_id: String, volume: f32, offset_ms: i32) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let shared = Arc::new(Shared {
            ring: Mutex::new(AudioRing::new()),
            volume: AtomicU32::new(volume.clamp(0.0, 1.0).to_bits()),
            offset_ms: AtomicI32::new(offset_ms),
            status: Mutex::new(AudioStatus::Starting),
            tap: Mutex::new(None),
        });

        let spawn = |name: &str, body: fn(&Shared, &AtomicBool, &str), id: String| {
            let (shared, stop) = (Arc::clone(&shared), Arc::clone(&stop));
            thread::Builder::new()
                .name(name.into())
                .spawn(move || body(&shared, &stop, &id))
                .ok()
        };
        let threads = [
            spawn("audio-capture", capture_thread, capture_device_id),
            spawn("audio-render", render_thread, String::new()),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

        if threads.len() < 2 {
            shared.set_status(AudioStatus::Failed("could not spawn audio threads".into()));
            stop.store(true, Ordering::Relaxed);
        }

        Self {
            stop,
            shared,
            threads,
        }
    }

    /// Starts (or with `None`, stops) feeding every captured block to `tap`.
    /// The tap receives the audio before the volume control, so recordings do
    /// not depend on how loud it is being played.
    pub fn set_tap(&self, tap: Option<Arc<AudioTap>>) {
        *self.shared.tap.lock().unwrap_or_else(|e| e.into_inner()) = tap;
    }

    pub fn set_volume(&self, volume: f32) {
        self.shared
            .volume
            .store(volume.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    pub fn set_offset_ms(&self, offset_ms: i32) {
        self.shared.offset_ms.store(offset_ms, Ordering::Relaxed);
    }

    pub fn status(&self) -> AudioStatus {
        self.shared
            .status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Audio currently buffered, in milliseconds (diagnostic).
    pub fn buffered_ms(&self) -> u32 {
        (self.shared.ring().frames() as u64 * 1000 / u64::from(SAMPLE_RATE)) as u32
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let deadline = Instant::now() + STOP_TIMEOUT;
        for thread in self.threads.drain(..) {
            while !thread.is_finished() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(5));
            }
            if thread.is_finished() {
                let _ = thread.join();
            }
        }
    }
}

/// The audio input Windows treats as the default, if there is one.
pub fn default_capture_device() -> Option<AudioDevice> {
    let _ = initialize_mta();
    let device = DeviceEnumerator::new()
        .ok()?
        .get_default_device(Direction::Capture)
        .ok()?;
    Some(AudioDevice {
        friendly_name: device.get_friendlyname().ok()?,
        device_id: device.get_id().ok()?,
    })
}

/// Lists the active audio capture endpoints.
pub fn enumerate_audio_devices() -> Result<Vec<AudioDevice>, WasapiError> {
    let _ = initialize_mta();
    let enumerator = DeviceEnumerator::new()?;
    let collection = enumerator.get_device_collection(Direction::Capture)?;
    let mut devices = Vec::new();
    for index in 0..collection.get_nbr_devices()? {
        let device = collection.get_device_at_index(index)?;
        devices.push(AudioDevice {
            friendly_name: device.get_friendlyname()?,
            device_id: device.get_id()?,
        });
    }
    Ok(devices)
}

pub(crate) fn format() -> WaveFormat {
    WaveFormat::new(
        32,
        32,
        SampleType::Float,
        SAMPLE_RATE as usize,
        CHANNELS,
        None,
    )
}

/// Opens `client` for event-driven shared-mode streaming in our fixed format.
pub(crate) fn init_stream(
    client: &mut AudioClient,
    direction: Direction,
    use_min_period: bool,
) -> Result<(), WasapiError> {
    let (default_period, min_period) = client.get_device_period()?;
    let mode = StreamMode::EventsShared {
        autoconvert: true,
        buffer_duration_hns: if use_min_period {
            min_period
        } else {
            default_period
        },
    };
    client.initialize_client(&format(), direction, mode, None)
}

fn capture_thread(shared: &Shared, stop: &AtomicBool, device_id: &str) {
    if let Err(e) = capture_loop(shared, stop, device_id) {
        if !stop.load(Ordering::Relaxed) {
            shared.set_status(AudioStatus::Disconnected(e.to_string()));
        }
    }
}

fn capture_loop(shared: &Shared, stop: &AtomicBool, device_id: &str) -> Result<(), WasapiError> {
    let _ = initialize_mta();
    let enumerator = DeviceEnumerator::new()?;
    let device = enumerator.get_device(device_id)?;
    let mut client = device.get_iaudioclient()?;
    init_stream(&mut client, Direction::Capture, true)?;
    let event = client.set_get_eventhandle()?;
    let capture = client.get_audiocaptureclient()?;
    client.start_stream()?;
    shared.set_status(AudioStatus::Running);

    let mut bytes = vec![0u8; 0];
    let mut samples: Vec<f32> = Vec::new();
    while !stop.load(Ordering::Relaxed) {
        // A timeout only means no packet arrived in this window.
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
            shared.ring().push(&samples);
            if let Some(tap) = shared
                .tap
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_ref()
            {
                tap.offer(Instant::now(), &samples);
            }
        }
    }
    let _ = client.stop_stream();
    Ok(())
}

fn render_thread(shared: &Shared, stop: &AtomicBool, _unused: &str) {
    if let Err(e) = render_loop(shared, stop) {
        if !stop.load(Ordering::Relaxed) {
            shared.set_status(AudioStatus::Failed(e.to_string()));
        }
    }
}

fn render_loop(shared: &Shared, stop: &AtomicBool) -> Result<(), WasapiError> {
    let _ = initialize_mta();
    let enumerator = DeviceEnumerator::new()?;
    let device = enumerator.get_default_device(Direction::Render)?;
    let mut client = device.get_iaudioclient()?;
    init_stream(&mut client, Direction::Render, false)?;
    let event = client.set_get_eventhandle()?;
    let render = client.get_audiorenderclient()?;
    client.start_stream()?;

    let mut out: Vec<f32> = Vec::new();
    let mut bytes: Vec<u8> = Vec::new();
    let mut primed = false;
    let mut last_drift_check = Instant::now();

    while !stop.load(Ordering::Relaxed) {
        if event.wait_for_event(EVENT_TIMEOUT_MS).is_err() {
            continue;
        }
        let space = client.get_available_space_in_frames()? as usize;
        if space == 0 {
            continue;
        }

        out.clear();
        out.resize(space * CHANNELS, 0.0);
        {
            let mut ring = shared.ring();
            let target = target_frames(shared.offset_ms.load(Ordering::Relaxed));

            // Pre-buffer to the target depth before playing, and again after
            // an underrun, so playback starts with a cushion.
            if !primed && ring.frames() >= target {
                primed = true;
            }
            if primed {
                let got = ring.pop_into(&mut out, shared.volume());
                if got < space {
                    primed = false; // underrun: the rest stays silent
                }
            }
            if primed && last_drift_check.elapsed() >= DRIFT_CHECK_INTERVAL {
                last_drift_check = Instant::now();
                let correction = plan_correction(ring.frames(), target);
                ring.apply(correction);
            }
        }

        bytes.clear();
        bytes.extend(out.iter().flat_map(|s| s.to_le_bytes()));
        render.write_to_device(&bytes, None)?;
    }
    let _ = client.stop_stream();
    Ok(())
}
