//! Lists audio capture endpoints, picks the one matching the video device
//! (or the first), and runs the audio pipeline for a few seconds.
//!
//! Run with: `cargo run -p dockview-core --example audio_probe [seconds]`

use std::thread;
use std::time::{Duration, Instant};

use dockview_domain::device::match_audio_to_video;
use dockview_platform::audio::{enumerate_audio_devices, AudioEngine};
use dockview_platform::mf::{enumerate_video_devices, MfSession};

fn main() {
    let seconds: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(5);

    let audio = enumerate_audio_devices().unwrap_or_else(|e| {
        println!("could not list audio devices: {e}");
        Vec::new()
    });
    println!("Audio capture devices:");
    for d in &audio {
        println!("  {}  [{}]", d.friendly_name, d.device_id);
    }

    let video = thread::spawn(|| {
        let session = MfSession::start().ok()?;
        enumerate_video_devices(&session).ok()?.into_iter().next()
    })
    .join()
    .ok()
    .flatten();
    println!(
        "Video device: {:?}",
        video.as_ref().map(|v| &v.friendly_name)
    );

    let chosen = match_audio_to_video(video.as_ref(), &audio).or(audio.first());
    let Some(chosen) = chosen else {
        println!("No audio capture device to use.");
        return;
    };
    println!("Using: {}", chosen.friendly_name);

    let engine = AudioEngine::start(chosen.device_id.clone(), 0.0, 0);
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(seconds) {
        println!(
            "t={}s status={:?} buffered={} ms",
            start.elapsed().as_secs(),
            engine.status(),
            engine.buffered_ms()
        );
        thread::sleep(Duration::from_secs(1));
    }
}
