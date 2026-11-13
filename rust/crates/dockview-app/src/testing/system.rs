//! Fakes of the small ports: what the PC says about itself, where the
//! measurements are logged and the window.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use dockview_domain::ports::{Host, StatsSink, SystemProbe};
use dockview_domain::settings::RenderPreset;
use dockview_domain::stats::{ProcessUsage, Stats};

/// A PC with 16 GB of RAM and a quiet process.
pub struct FakeSystem {
    pub usage: Mutex<ProcessUsage>,
    pub ram_bytes: u64,
}

impl Default for FakeSystem {
    fn default() -> Self {
        Self {
            usage: Mutex::new(ProcessUsage {
                cpu_core_percent: 5.0,
                cpu_total_percent: 1.0,
                memory_mb: 200.0,
            }),
            ram_bytes: 16 << 30,
        }
    }
}

impl SystemProbe for FakeSystem {
    fn sample_usage(&self) -> ProcessUsage {
        *self.usage.lock().unwrap()
    }
    fn total_ram_bytes(&self) -> u64 {
        self.ram_bytes
    }
}

/// Keeps every measurement it is given.
#[derive(Clone, Default)]
pub struct FakeStats(pub Arc<Mutex<Vec<Stats>>>);

impl StatsSink for FakeStats {
    fn record(&mut self, stats: &Stats) {
        self.0.lock().unwrap().push(stats.clone());
    }
}

/// What the code under test asked of the window, and what the window answers.
pub struct HostWorld {
    pub fullscreen: Vec<bool>,
    pub presets: Vec<RenderPreset>,
    pub exclusions: Vec<bool>,
    pub redraws: u32,
    pub picked_folder: Option<PathBuf>,
    pub window_size: (u32, u32),
    pub monitor_hz: Option<f32>,
    pub gpu_ms: Option<f32>,
}

impl Default for HostWorld {
    fn default() -> Self {
        Self {
            fullscreen: Vec::new(),
            presets: Vec::new(),
            exclusions: Vec::new(),
            redraws: 0,
            picked_folder: None,
            window_size: (1280, 720),
            monitor_hz: Some(60.0),
            gpu_ms: Some(1.0),
        }
    }
}

pub type SharedHost = Arc<Mutex<HostWorld>>;

/// A window that writes down every request.
pub struct RecordingHost(pub SharedHost);

impl Host for RecordingHost {
    fn set_fullscreen(&self, on: bool) {
        self.0.lock().unwrap().fullscreen.push(on);
    }
    fn set_render_preset(&self, preset: RenderPreset) {
        self.0.lock().unwrap().presets.push(preset);
    }
    fn exclude_from_capture(&self, on: bool) {
        self.0.lock().unwrap().exclusions.push(on);
    }
    fn pick_folder(&self, _: &Path) -> Option<PathBuf> {
        self.0.lock().unwrap().picked_folder.clone()
    }
    fn window_size(&self) -> (u32, u32) {
        self.0.lock().unwrap().window_size
    }
    fn monitor_hz(&self) -> Option<f32> {
        self.0.lock().unwrap().monitor_hz
    }
    fn gpu_ms(&self) -> Option<f32> {
        self.0.lock().unwrap().gpu_ms
    }
    fn request_redraw(&self) {
        self.0.lock().unwrap().redraws += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_system_reports_what_it_was_given() {
        let system = FakeSystem::default();
        assert_eq!(system.total_ram_bytes(), 16 << 30);
        system.usage.lock().unwrap().cpu_core_percent = 80.0;
        assert_eq!(system.sample_usage().cpu_core_percent, 80.0);
    }

    #[test]
    fn the_stats_sink_keeps_what_it_receives() {
        let mut sink = FakeStats::default();
        let log = Arc::clone(&sink.0);
        sink.record(&Stats::default());
        sink.record(&Stats::default());
        assert_eq!(log.lock().unwrap().len(), 2);
    }

    #[test]
    fn the_host_writes_down_requests_in_order() {
        let world = SharedHost::default();
        let host = RecordingHost(Arc::clone(&world));
        host.set_fullscreen(true);
        host.set_fullscreen(false);
        host.request_redraw();
        assert_eq!(host.window_size(), (1280, 720));
        let w = world.lock().unwrap();
        assert_eq!(w.fullscreen, [true, false]);
        assert_eq!(w.redraws, 1);
    }
}
