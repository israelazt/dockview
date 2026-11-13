//! The once-a-second measurements: frames received, shown and dropped per
//! second, the average time a picture spent in the app, and the process's CPU
//! and memory.
//!
//! It only counts. The window, the GPU and the source are asked by the app,
//! which then builds the [`Stats`] it shows.

use std::time::{Duration, Instant};

use dockview_domain::diagnostics::Finding;

use dockview_platform::perf::{ProcessSampler, ProcessUsage};

/// How often a new set of measurements is made.
const WINDOW: Duration = Duration::from_secs(1);

/// Everything measured about the running app, for the stats panel and the
/// log. Unknown values are `None`; the panel shows them as "Measuring…" or
/// leaves them out.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Stats {
    /// Formatted device mode, e.g. `NV12 · 1920×1080 · 30fps`.
    pub device_mode: Option<String>,
    /// Data rate of the mode when sent uncompressed.
    pub mode_mbps: Option<f32>,
    pub received_fps: f32,
    pub shown_fps: f32,
    pub dropped_per_sec: f32,
    /// Time from a frame reaching the app until it was handed to the screen.
    pub latency_ms: Option<f32>,
    pub cpu_core_percent: f32,
    pub cpu_total_percent: f32,
    pub gpu_ms: Option<f32>,
    pub memory_mb: f32,
    pub audio_buffer_ms: Option<u32>,
    pub monitor_hz: Option<f32>,
    pub window: (u32, u32),
    pub findings: Vec<Finding>,
    /// `false` until the first full second of measurements exists.
    pub measured: bool,
}

/// Frames per second over the last window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rates {
    pub received_fps: f32,
    pub dropped_per_sec: f32,
    pub shown_fps: f32,
}

pub struct Telemetry {
    sampler: ProcessSampler,
    window_start: Instant,
    last_published: u64,
    last_dropped: u64,
    last_rendered: u64,
    latency_sum_ms: f64,
    latency_count: u32,
    windows: u32,
    shown_fps: f32,
}

impl Telemetry {
    pub fn new() -> Self {
        Self {
            sampler: ProcessSampler::new(),
            window_start: Instant::now(),
            last_published: 0,
            last_dropped: 0,
            last_rendered: 0,
            latency_sum_ms: 0.0,
            latency_count: 0,
            windows: 0,
            shown_fps: 0.0,
        }
    }

    /// One picture took `ms` from arriving to being on screen.
    pub fn note_latency(&mut self, ms: f64) {
        self.latency_sum_ms += ms;
        self.latency_count += 1;
    }

    /// How many windows were closed so far (the first ones include start-up
    /// work, so screenshots wait for a few).
    pub fn windows(&self) -> u32 {
        self.windows
    }

    /// Frames shown per second in the last window.
    pub fn shown_fps(&self) -> f32 {
        self.shown_fps
    }

    /// Closes the window if a second has passed since the last one, given the
    /// running totals of pictures published, dropped and rendered. Returns
    /// the rates over that window.
    pub fn close_window(
        &mut self,
        now: Instant,
        published: u64,
        dropped: u64,
        rendered: u64,
    ) -> Option<Rates> {
        let elapsed = now.saturating_duration_since(self.window_start);
        if elapsed < WINDOW {
            return None;
        }
        let secs = elapsed.as_secs_f32();
        let per_sec = |now: u64, before: u64| now.saturating_sub(before) as f32 / secs;
        let rates = Rates {
            received_fps: per_sec(published, self.last_published),
            dropped_per_sec: per_sec(dropped, self.last_dropped),
            shown_fps: per_sec(rendered, self.last_rendered),
        };
        self.last_published = published;
        self.last_dropped = dropped;
        self.last_rendered = rendered;
        self.window_start = now;
        self.windows += 1;
        self.shown_fps = rates.shown_fps;
        Some(rates)
    }

    /// CPU and memory of the process since the last call.
    pub fn usage(&mut self) -> ProcessUsage {
        self.sampler.sample()
    }

    /// The average latency of the window, then starts a new one. With no
    /// pictures in the window it keeps `previous`.
    pub fn take_latency(&mut self, previous: Option<f32>) -> Option<f32> {
        let average = (self.latency_count > 0)
            .then(|| (self.latency_sum_ms / f64::from(self.latency_count)) as f32)
            .or(previous);
        self.latency_sum_ms = 0.0;
        self.latency_count = 0;
        average
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(telemetry: &Telemetry, secs: f32) -> Instant {
        telemetry.window_start + Duration::from_secs_f32(secs)
    }

    #[test]
    fn nothing_is_measured_before_a_second_has_passed() {
        let mut t = Telemetry::new();
        let now = at(&t, 0.5);
        assert_eq!(t.close_window(now, 30, 0, 30), None);
        assert_eq!(t.windows(), 0);
    }

    #[test]
    fn rates_are_the_change_in_the_totals_over_the_window() {
        let mut t = Telemetry::new();
        let now = at(&t, 2.0);
        let rates = t.close_window(now, 120, 10, 100).expect("a window");
        assert_eq!(rates.received_fps, 60.0);
        assert_eq!(rates.dropped_per_sec, 5.0);
        assert_eq!(rates.shown_fps, 50.0);
        assert_eq!(t.shown_fps(), 50.0);
        assert_eq!(t.windows(), 1);

        // The next window counts from the new totals.
        let later = now + Duration::from_secs(1);
        let rates = t.close_window(later, 150, 10, 130).expect("another");
        assert_eq!(rates.received_fps, 30.0);
        assert_eq!(rates.dropped_per_sec, 0.0);
        assert_eq!(t.windows(), 2);
    }

    #[test]
    fn totals_that_go_backwards_do_not_underflow() {
        // A new source starts counting from zero again.
        let mut t = Telemetry::new();
        t.close_window(at(&t, 1.0), 500, 20, 500).expect("first");
        let rates = t
            .close_window(at(&t, 1.0), 10, 0, 10)
            .expect("second after a restart");
        assert_eq!(rates.received_fps, 0.0);
    }

    #[test]
    fn latency_is_the_average_and_a_quiet_window_keeps_the_last_value() {
        let mut t = Telemetry::new();
        t.note_latency(4.0);
        t.note_latency(8.0);
        assert_eq!(t.take_latency(Some(99.0)), Some(6.0));
        // Nothing arrived in the next window: the previous figure stays.
        assert_eq!(t.take_latency(Some(6.0)), Some(6.0));
        assert_eq!(t.take_latency(None), None);
    }
}
