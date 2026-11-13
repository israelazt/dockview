//! Lists the audio inputs and measures each one's level for a couple of
//! seconds (speak while it runs), plus the Windows default microphone.
//!
//! Run with: `cargo run --release -p dockview-core --example mic_level`

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use dockview_domain::pcm::AudioTap;
use dockview_platform::audio::enumerate_audio_devices;
use dockview_platform::tap_capture::{CaptureSource, TapCapture};

fn measure(label: &str, source: CaptureSource) {
    let tap = Arc::new(AudioTap::new(64));
    let capture = TapCapture::start(source, 1.0, Arc::clone(&tap));
    let mut peak = 0.0f32;
    for _ in 0..30 {
        thread::sleep(Duration::from_millis(100));
        peak = peak.max(capture.level());
    }
    println!("{label:<55} {:?}  peak level {:.4}", capture.status(), peak);
}

fn default_name() -> String {
    use wasapi::{initialize_mta, DeviceEnumerator, Direction};
    let _ = initialize_mta();
    DeviceEnumerator::new()
        .and_then(|e| e.get_default_device(Direction::Capture))
        .and_then(|d| d.get_friendlyname())
        .unwrap_or_else(|e| format!("unknown ({e})"))
}

fn main() {
    println!("Windows default input: {}", default_name());
    let devices = enumerate_audio_devices().unwrap_or_default();
    println!(
        "Speak now. Measuring {} inputs for 3 s each...\n",
        devices.len() + 1
    );
    measure("(Windows default microphone)", CaptureSource::Device(None));
    for device in devices {
        measure(
            &device.friendly_name,
            CaptureSource::Device(Some(device.device_id)),
        );
    }
}
