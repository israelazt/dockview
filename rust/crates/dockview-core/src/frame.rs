//! Video frames and the "latest frame wins" hand-off between capture and render.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

/// Pixel layout of a [`Frame`]. Colour conversion happens on the GPU.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    /// Planar 4:2:0: a `height`-row Y plane followed by `height / 2` rows of
    /// interleaved UV.
    Nv12,
    /// Packed 4:2:2: `Y0 U Y1 V` per pair of pixels.
    Yuy2,
}

impl PixelFormat {
    /// Bytes needed for a frame with the given row `stride` and `height`.
    pub fn buffer_len(self, stride: usize, height: usize) -> usize {
        match self {
            Self::Nv12 => stride * height * 3 / 2,
            Self::Yuy2 => stride * height,
        }
    }
}

/// One decoded video frame. `stride` is the number of bytes per row of the
/// first plane, which can exceed the visible width.
#[derive(Debug, Clone)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub format: PixelFormat,
    pub data: Vec<u8>,
    /// Presentation time in 100 ns units, as reported by Media Foundation.
    pub timestamp: i64,
    /// When the frame reached the application (set by [`FrameSlot::publish`]).
    /// Used to measure the app's own latency.
    pub arrived: Instant,
}

#[derive(Default)]
struct Slots {
    latest: Option<Frame>,
    spare: Option<Frame>,
}

/// Single-slot mailbox between the capture thread and the renderer.
///
/// There is deliberately no queue: publishing replaces any frame that was not
/// consumed yet, so latency never accumulates. Spent frames are recycled so
/// the capture thread can reuse their buffers instead of allocating per frame.
#[derive(Default)]
pub struct FrameSlot {
    slots: Mutex<Slots>,
    arrived: Condvar,
    published: AtomicU64,
    dropped: AtomicU64,
}

impl FrameSlot {
    pub fn new() -> Self {
        Self::default()
    }

    /// Stores `frame` as the latest one, dropping any unconsumed predecessor.
    pub fn publish(&self, mut frame: Frame) {
        frame.arrived = Instant::now();
        let mut slots = self.lock();
        if let Some(old) = slots.latest.replace(frame) {
            slots.spare = Some(old);
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
        self.published.fetch_add(1, Ordering::Relaxed);
        drop(slots);
        self.arrived.notify_one();
    }

    /// Blocks until a frame is available or `timeout` elapses. Returns whether
    /// a frame is waiting; it is not consumed.
    pub fn wait_for_frame(&self, timeout: Duration) -> bool {
        let guard = self.lock();
        let (guard, _) = self
            .arrived
            .wait_timeout_while(guard, timeout, |s| s.latest.is_none())
            .unwrap_or_else(|e| e.into_inner());
        guard.latest.is_some()
    }

    /// Takes the latest frame, if one arrived since the last call.
    pub fn take_latest(&self) -> Option<Frame> {
        self.lock().latest.take()
    }

    /// Returns a consumed frame so its buffer can be reused.
    pub fn recycle(&self, frame: Frame) {
        self.lock().spare = Some(frame);
    }

    /// Takes a spare frame whose buffer can be reused for the next capture.
    pub fn take_spare(&self) -> Option<Frame> {
        self.lock().spare.take()
    }

    /// Total frames published so far.
    pub fn published(&self) -> u64 {
        self.published.load(Ordering::Relaxed)
    }

    /// Frames replaced before the renderer consumed them.
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Slots> {
        // A poisoned lock only means another thread panicked mid-swap; the
        // slots are still structurally valid, so keep going.
        self.slots.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(timestamp: i64) -> Frame {
        Frame {
            width: 4,
            height: 2,
            stride: 4,
            format: PixelFormat::Nv12,
            data: vec![0; 12],
            timestamp,
            arrived: Instant::now(),
        }
    }

    #[test]
    fn publishing_stamps_the_arrival_time() {
        let slot = FrameSlot::new();
        let mut old = frame(1);
        old.arrived = Instant::now() - Duration::from_secs(60);
        slot.publish(old);
        let got = slot.take_latest().unwrap();
        assert!(got.arrived.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn empty_slot_yields_nothing() {
        assert!(FrameSlot::new().take_latest().is_none());
    }

    #[test]
    fn take_latest_consumes_the_frame() {
        let slot = FrameSlot::new();
        slot.publish(frame(1));
        assert_eq!(slot.take_latest().unwrap().timestamp, 1);
        assert!(slot.take_latest().is_none());
    }

    #[test]
    fn newest_frame_wins_and_older_is_counted_as_dropped() {
        let slot = FrameSlot::new();
        slot.publish(frame(1));
        slot.publish(frame(2));
        slot.publish(frame(3));
        assert_eq!(slot.take_latest().unwrap().timestamp, 3);
        assert_eq!(slot.published(), 3);
        assert_eq!(slot.dropped(), 2);
    }

    #[test]
    fn replaced_frame_becomes_a_reusable_spare() {
        let slot = FrameSlot::new();
        slot.publish(frame(1));
        slot.publish(frame(2));
        assert_eq!(slot.take_spare().unwrap().timestamp, 1);
        assert!(slot.take_spare().is_none());
    }

    #[test]
    fn recycled_frame_is_available_as_spare() {
        let slot = FrameSlot::new();
        slot.recycle(frame(7));
        assert_eq!(slot.take_spare().unwrap().timestamp, 7);
    }

    #[test]
    fn wait_returns_immediately_when_a_frame_is_waiting() {
        let slot = FrameSlot::new();
        slot.publish(frame(1));
        assert!(slot.wait_for_frame(Duration::from_secs(5)));
    }

    #[test]
    fn wait_times_out_when_no_frame_arrives() {
        assert!(!FrameSlot::new().wait_for_frame(Duration::from_millis(5)));
    }

    #[test]
    fn wait_wakes_when_a_frame_is_published() {
        let slot = std::sync::Arc::new(FrameSlot::new());
        let producer = {
            let slot = std::sync::Arc::clone(&slot);
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(20));
                slot.publish(frame(9));
            })
        };
        assert!(slot.wait_for_frame(Duration::from_secs(5)));
        producer.join().unwrap();
    }

    #[test]
    fn buffer_len_matches_layout() {
        assert_eq!(
            PixelFormat::Nv12.buffer_len(1920, 1080),
            1920 * 1080 * 3 / 2
        );
        assert_eq!(PixelFormat::Yuy2.buffer_len(3840, 1080), 3840 * 1080);
    }
}
