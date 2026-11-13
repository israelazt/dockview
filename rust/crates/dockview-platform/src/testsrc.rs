//! Synthetic video source: SMPTE-style colour bars with a moving stripe.
//!
//! Lets the render pipeline be exercised and verified without capture
//! hardware or a live HDMI signal.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::worker::Worker;
use dockview_domain::frame::{Frame, FrameSlot, PixelFormat};

/// How long dropping the source waits for its thread.
const STOP_TIMEOUT: Duration = Duration::from_secs(2);

/// Bar colours, left to right (white, yellow, cyan, green, magenta, red, blue, black).
const BARS: [(u8, u8, u8); 8] = [
    (255, 255, 255),
    (255, 255, 0),
    (0, 255, 255),
    (0, 255, 0),
    (255, 0, 255),
    (255, 0, 0),
    (0, 0, 255),
    (0, 0, 0),
];

/// Converts full-range RGB to limited-range BT.709 YUV.
pub fn rgb_to_yuv709(r: u8, g: u8, b: u8) -> (u8, u8, u8) {
    let (r, g, b) = (
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
    );
    let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    let u = (b - y) / 1.8556;
    let v = (r - y) / 1.5748;
    let quantise = |value: f32| value.round().clamp(0.0, 255.0) as u8;
    (
        quantise(16.0 + 219.0 * y),
        quantise(128.0 + 224.0 * u),
        quantise(128.0 + 224.0 * v),
    )
}

/// Builds the static NV12 colour-bar image (tightly packed, stride = width).
pub fn nv12_bars(width: u32, height: u32) -> Frame {
    let (w, h) = (width as usize, height as usize);
    let mut data = vec![0u8; PixelFormat::Nv12.buffer_len(w, h)];
    let bar_of = |x: usize| BARS[(x * BARS.len() / w).min(BARS.len() - 1)];

    for row in data[..w * h].chunks_exact_mut(w) {
        for (x, y) in row.iter_mut().enumerate() {
            let (r, g, b) = bar_of(x);
            *y = rgb_to_yuv709(r, g, b).0;
        }
    }
    for row in data[w * h..].chunks_exact_mut(w) {
        let (pairs, _) = row.as_chunks_mut::<2>();
        for (pair, uv) in pairs.iter_mut().enumerate() {
            let (r, g, b) = bar_of(pair * 2);
            let (_, u, v) = rgb_to_yuv709(r, g, b);
            *uv = [u, v];
        }
    }

    Frame {
        width,
        height,
        stride: width,
        format: PixelFormat::Nv12,
        data,
        timestamp: 0,
        arrived: std::time::Instant::now(),
    }
}

/// Draws a white vertical stripe whose position depends on `tick`, so motion
/// and frame pacing are visible. Operates on the luma plane only.
pub fn overlay_stripe(frame: &mut Frame, tick: u64) {
    let (w, h, stride) = (
        frame.width as usize,
        frame.height as usize,
        frame.stride as usize,
    );
    let stripe_w = (w / 16).max(2);
    let x0 = (tick as usize * 12) % w.saturating_sub(stripe_w).max(1);
    for row in 0..h {
        let start = row * stride + x0;
        frame.data[start..start + stripe_w].fill(235);
    }
    // Neutral chroma under the stripe so it stays white.
    for row in 0..h / 2 {
        let start = stride * h + row * stride + (x0 & !1);
        let end = (start + stripe_w + 2).min(stride * h + (row + 1) * stride);
        frame.data[start..end].fill(128);
    }
}

/// Background thread that publishes the test pattern at a fixed frame rate.
pub struct TestSource {
    _worker: Worker,
}

impl TestSource {
    pub fn start(slot: Arc<FrameSlot>, width: u32, height: u32, fps: u32) -> Self {
        let mut worker = Worker::new(STOP_TIMEOUT);
        worker.spawn("test-source", move |stop| {
            run(&slot, width, height, fps, stop);
        });
        Self { _worker: worker }
    }
}

fn run(slot: &FrameSlot, width: u32, height: u32, fps: u32, stop: &AtomicBool) {
    let base = nv12_bars(width, height);
    let period = Duration::from_secs_f64(1.0 / f64::from(fps.max(1)));
    let start = Instant::now();
    let mut tick = 0u64;

    while !stop.load(Ordering::Relaxed) {
        let mut frame = slot.take_spare().unwrap_or_else(|| base.clone());
        frame.data.clear();
        frame.data.extend_from_slice(&base.data);
        frame.width = base.width;
        frame.height = base.height;
        frame.stride = base.stride;
        frame.format = PixelFormat::Nv12;
        frame.timestamp = (start.elapsed().as_nanos() / 100) as i64;
        overlay_stripe(&mut frame, tick);
        slot.publish(frame);

        tick += 1;
        let next = start + period.mul_f64(tick as f64);
        if let Some(wait) = next.checked_duration_since(Instant::now()) {
            thread::sleep(wait);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn white_and_black_map_to_limited_range_extremes() {
        assert_eq!(rgb_to_yuv709(255, 255, 255), (235, 128, 128));
        assert_eq!(rgb_to_yuv709(0, 0, 0), (16, 128, 128));
    }

    #[test]
    fn primary_red_has_high_v_and_low_u() {
        let (_, u, v) = rgb_to_yuv709(255, 0, 0);
        assert!(v > 200 && u < 128);
    }

    #[test]
    fn bars_frame_has_nv12_layout() {
        let f = nv12_bars(64, 32);
        assert_eq!(f.data.len(), 64 * 32 * 3 / 2);
        assert_eq!(f.data[0], 235); // first bar is white
        assert_eq!(f.data[63], 16); // last bar is black
    }

    #[test]
    fn stripe_moves_between_ticks_without_leaving_the_frame() {
        let base = nv12_bars(64, 32);
        let mut a = base.clone();
        let mut b = base.clone();
        overlay_stripe(&mut a, 0);
        overlay_stripe(&mut b, 3);
        assert_ne!(a.data, b.data);
        assert_eq!(a.data.len(), base.data.len());
    }

    #[test]
    fn source_publishes_frames_until_dropped() {
        let slot = Arc::new(FrameSlot::new());
        let source = TestSource::start(Arc::clone(&slot), 64, 32, 200);
        assert!(slot.wait_for_frame(Duration::from_secs(2)));
        drop(source);
        assert!(slot.published() >= 1);
    }
}
