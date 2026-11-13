//! What the app measures about itself, for the statistics panel and the log.

use crate::diagnostics::Finding;

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

/// What the app is costing the machine right now.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ProcessUsage {
    /// Percent of one core.
    pub cpu_core_percent: f32,
    /// Percent of the whole CPU (all logical cores).
    pub cpu_total_percent: f32,
    pub memory_mb: f32,
}
