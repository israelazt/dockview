//! Video frames and the "latest frame wins" hand-off between capture and render.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
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

/// The colour matrix that turns a frame's YUV into RGB.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorMatrix {
    /// The device did not say: BT.709 for HD pictures, BT.601 below that.
    #[default]
    Auto,
    Bt601,
    Bt709,
}

impl ColorMatrix {
    /// The matrix to use for a picture `height` lines tall (never `Auto`).
    pub fn resolve(self, height: u32) -> ColorMatrix {
        match self {
            Self::Auto if height >= 720 => Self::Bt709,
            Self::Auto => Self::Bt601,
            fixed => fixed,
        }
    }
}

/// Whether the luma and chroma use the whole 0-255 range or the video range
/// (16-235 and 16-240).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorRange {
    #[default]
    Limited,
    Full,
}

/// How the colours of a frame are coded. What the device does not say is left
/// at the usual defaults (see [`ColorMatrix::resolve`] and limited range).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ColorInfo {
    pub matrix: ColorMatrix,
    pub range: ColorRange,
}

/// One decoded video frame. `stride` is the number of bytes per row of the
/// first plane, which can exceed the visible width.
#[derive(Debug, Clone)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub format: PixelFormat,
    /// How the colours are coded.
    pub color: ColorInfo,
    pub data: Vec<u8>,
    /// Presentation time in 100 ns units, as reported by Media Foundation.
    pub timestamp: i64,
    /// When the frame reached the application (set by [`FrameSlot::publish`]).
    /// Used to measure the app's own latency.
    pub arrived: Instant,
}

/// A second consumer of the frame stream that must see *every* frame (the
/// recorder), unlike the display, which only wants the newest.
///
/// Frames are copied into a small bounded queue. If the consumer falls behind
/// the newest frames are dropped and counted, so a slow recorder can never
/// slow the capture down.
pub struct FrameTap {
    queue: Mutex<VecDeque<Frame>>,
    ready: Condvar,
    /// Buffers returned by the consumer, reused for the next copies.
    pool: Mutex<Vec<Vec<u8>>>,
    capacity: usize,
    dropped: AtomicU64,
}

impl FrameTap {
    pub fn new(capacity: usize) -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            ready: Condvar::new(),
            pool: Mutex::new(Vec::new()),
            capacity: capacity.max(1),
            dropped: AtomicU64::new(0),
        }
    }

    /// Copies `frame` into the queue (or counts a drop when it is full).
    pub fn offer(&self, frame: &Frame) {
        let mut queue = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        if queue.len() >= self.capacity {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return;
        }
        let mut data = self
            .pool
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pop()
            .unwrap_or_default();
        data.clear();
        data.extend_from_slice(&frame.data);
        queue.push_back(Frame {
            data,
            ..frame.clone_header()
        });
        drop(queue);
        self.ready.notify_one();
    }

    /// Waits up to `timeout` for the oldest queued frame.
    pub fn take(&self, timeout: Duration) -> Option<Frame> {
        let queue = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        let (mut queue, _) = self
            .ready
            .wait_timeout_while(queue, timeout, |q| q.is_empty())
            .unwrap_or_else(|e| e.into_inner());
        queue.pop_front()
    }

    /// Returns a consumed frame's buffer for reuse.
    pub fn recycle(&self, frame: Frame) {
        let mut pool = self.pool.lock().unwrap_or_else(|e| e.into_inner());
        if pool.len() < self.capacity * 2 {
            pool.push(frame.data);
        }
    }

    /// Frames dropped because the consumer was too slow.
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Frames waiting to be consumed.
    pub fn pending(&self) -> usize {
        self.queue.lock().unwrap_or_else(|e| e.into_inner()).len()
    }
}

#[derive(Default)]
struct Slots {
    latest: Option<Frame>,
    spare: Option<Frame>,
}

impl Frame {
    /// A copy of everything but the pixel data (which is left empty).
    fn clone_header(&self) -> Frame {
        Frame {
            width: self.width,
            height: self.height,
            stride: self.stride,
            format: self.format,
            color: self.color,
            data: Vec::new(),
            timestamp: self.timestamp,
            arrived: self.arrived,
        }
    }
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
    tap: Mutex<Option<Arc<FrameTap>>>,
    /// While set, frames published here go to this slot instead.
    forward: Mutex<Option<Arc<FrameSlot>>>,
    published: AtomicU64,
    dropped: AtomicU64,
}

impl FrameSlot {
    pub fn new() -> Self {
        Self::default()
    }

    /// Hands every frame published here to `target` from now on (or, with
    /// `None`, keeps them here again). It lets a source keep one slot while
    /// the thing reading it changes, so nothing has to restart.
    pub fn set_forward(&self, target: Option<Arc<FrameSlot>>) {
        *self.forward.lock().unwrap_or_else(|e| e.into_inner()) = target;
    }

    fn forward_target(&self) -> Option<Arc<FrameSlot>> {
        self.forward
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Stores `frame` as the latest one, dropping any unconsumed predecessor.
    pub fn publish(&self, mut frame: Frame) {
        if let Some(target) = self.forward_target() {
            target.publish(frame);
            return;
        }
        frame.arrived = Instant::now();
        let tap = self.tap.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if let Some(tap) = tap {
            tap.offer(&frame);
        }
        let mut slots = self.lock();
        if let Some(old) = slots.latest.replace(frame) {
            slots.spare = Some(old);
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
        self.published.fetch_add(1, Ordering::Relaxed);
        drop(slots);
        self.arrived.notify_one();
    }

    /// Starts (or with `None`, stops) feeding every published frame to `tap`.
    pub fn set_tap(&self, tap: Option<Arc<FrameTap>>) {
        *self.tap.lock().unwrap_or_else(|e| e.into_inner()) = tap;
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
        if let Some(target) = self.forward_target() {
            return target.take_spare();
        }
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
            color: ColorInfo::default(),
            data: vec![0; 12],
            timestamp,
            arrived: Instant::now(),
        }
    }

    #[test]
    fn an_unspecified_matrix_follows_the_height_and_a_stated_one_wins() {
        assert_eq!(ColorMatrix::Auto.resolve(1080), ColorMatrix::Bt709);
        assert_eq!(ColorMatrix::Auto.resolve(720), ColorMatrix::Bt709);
        assert_eq!(ColorMatrix::Auto.resolve(480), ColorMatrix::Bt601);
        assert_eq!(ColorMatrix::Bt601.resolve(1080), ColorMatrix::Bt601);
        assert_eq!(ColorMatrix::Bt709.resolve(480), ColorMatrix::Bt709);
        assert_eq!(ColorInfo::default().range, ColorRange::Limited);
    }

    #[test]
    fn a_tap_sees_every_frame_in_order() {
        let slot = FrameSlot::new();
        let tap = Arc::new(FrameTap::new(8));
        slot.set_tap(Some(Arc::clone(&tap)));
        for i in 0..5 {
            slot.publish(frame(i));
        }
        // The display only keeps the newest, the tap keeps all of them.
        assert_eq!(slot.take_latest().unwrap().timestamp, 4);
        let seen: Vec<i64> = (0..5)
            .map(|_| tap.take(Duration::from_millis(10)).unwrap().timestamp)
            .collect();
        assert_eq!(seen, vec![0, 1, 2, 3, 4]);
        assert!(tap.take(Duration::from_millis(5)).is_none());
    }

    #[test]
    fn a_full_tap_drops_new_frames_and_counts_them() {
        let slot = FrameSlot::new();
        let tap = Arc::new(FrameTap::new(2));
        slot.set_tap(Some(Arc::clone(&tap)));
        for i in 0..5 {
            slot.publish(frame(i));
        }
        assert_eq!((tap.pending(), tap.dropped()), (2, 3));
        assert_eq!(tap.take(Duration::from_millis(1)).unwrap().timestamp, 0);
    }

    #[test]
    fn a_tap_copies_the_pixels_and_reuses_returned_buffers() {
        let slot = FrameSlot::new();
        let tap = Arc::new(FrameTap::new(4));
        slot.set_tap(Some(Arc::clone(&tap)));
        let mut f = frame(1);
        f.data = vec![7; 12];
        slot.publish(f);
        let got = tap.take(Duration::from_millis(5)).unwrap();
        assert_eq!(got.data, vec![7; 12]);
        let capacity = got.data.capacity();
        tap.recycle(got);
        slot.publish(frame(2));
        assert!(tap.take(Duration::from_millis(5)).unwrap().data.capacity() >= capacity);
    }

    #[test]
    fn removing_the_tap_stops_the_copies() {
        let slot = FrameSlot::new();
        let tap = Arc::new(FrameTap::new(4));
        slot.set_tap(Some(Arc::clone(&tap)));
        slot.set_tap(None);
        slot.publish(frame(1));
        assert_eq!(tap.pending(), 0);
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

    #[test]
    fn a_forwarding_slot_hands_its_frames_on_and_can_stop_doing_so() {
        let source = FrameSlot::new();
        let target = Arc::new(FrameSlot::new());
        source.set_forward(Some(Arc::clone(&target)));
        source.publish(frame(1));
        assert!(source.take_latest().is_none(), "nothing is kept here");
        assert_eq!(target.take_latest().map(|f| f.timestamp), Some(1));

        // Buffers come back from where the frames went.
        target.recycle(frame(7));
        assert_eq!(source.take_spare().map(|f| f.timestamp), Some(7));

        // The target's tap sees frames that arrive through the forward.
        let tap = Arc::new(FrameTap::new(4));
        target.set_tap(Some(Arc::clone(&tap)));
        source.publish(frame(2));
        assert_eq!(tap.pending(), 1);

        // Frames stay here again once the forward is removed.
        source.set_forward(None);
        source.publish(frame(3));
        assert_eq!(source.take_latest().map(|f| f.timestamp), Some(3));
        assert!(target.take_latest().is_some_and(|f| f.timestamp == 2));
    }
}
