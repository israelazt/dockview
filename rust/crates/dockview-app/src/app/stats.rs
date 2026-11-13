//! Gathers the measurements the stats panel shows and acts on the automatic
//! mode switch.

use super::*;

impl App {
    /// Refreshes the measurements once a second.
    pub(super) fn update_stats(&mut self) {
        let Some(Rates {
            received_fps,
            dropped_per_sec,
            shown_fps,
        }) = self.telemetry.close_window(
            Instant::now(),
            self.slot.published(),
            self.slot.dropped(),
            self.rendered,
        )
        else {
            return;
        };

        let usage = self.telemetry.usage();
        let latency_ms = self.telemetry.take_latency(self.stats.latency_ms);

        let native = self.native_mode();
        let video = self.source.video_state();
        let monitor_hz = self
            .window
            .as_ref()
            .and_then(|w| w.current_monitor())
            .and_then(|m| m.refresh_rate_millihertz())
            .map(|mhz| mhz as f32 / 1000.0);
        let gpu_ms = self.renderer.as_ref().and_then(Renderer::gpu_ms);
        let audio_ok = self.audio.as_ref().map(|a| {
            !matches!(
                a.status(),
                AudioStatus::Disconnected(_) | AudioStatus::Failed(_)
            )
        });

        let findings = diagnose(&Snapshot {
            video,
            mode_fps: native.as_ref().map(|n| n.fps),
            mode_raw_mbps: native.as_ref().and_then(StreamInfo::raw_mbps),
            received_fps,
            dropped_per_sec,
            cpu_core_percent: usage.cpu_core_percent,
            gpu_ms,
            frame_budget_ms: monitor_hz.map(|hz| 1000.0 / hz),
            audio_ok,
        });

        let allowed = self.settings.video_mode == VideoModePref::Auto
            && video == VideoState::Running
            && self.capture_has_compressed()
            && !self.cli.test_pattern;
        let usb_slow = findings.contains(&Finding::NeedsUsb3);
        if let Some(event) = self.auto.tick(usb_slow, allowed, received_fps) {
            self.apply_switch_event(event);
        }
        let mut findings = findings;
        if self.auto.is_switched() {
            findings.insert(0, Finding::SwitchedToCompressed);
        }

        let findings = if self.cli.ui_preview && self.cli.demo_findings {
            use dockview_domain::diagnostics::Finding;
            vec![
                Finding::NeedsUsb3,
                Finding::DroppingFrames,
                Finding::HighCpu,
            ]
        } else {
            findings
        };

        let size = self
            .window
            .as_ref()
            .map_or((0, 0), |w| (w.inner_size().width, w.inner_size().height));
        self.stats = Stats {
            device_mode: native.as_ref().map(ToString::to_string),
            mode_mbps: native.as_ref().and_then(StreamInfo::raw_mbps),
            received_fps,
            shown_fps,
            dropped_per_sec,
            latency_ms,
            cpu_core_percent: usage.cpu_core_percent,
            cpu_total_percent: usage.cpu_total_percent,
            gpu_ms,
            memory_mb: usage.memory_mb,
            audio_buffer_ms: self.audio.as_ref().map(|a| a.buffered_ms()),
            monitor_hz,
            window: size,
            findings,
            measured: true,
        };
        if let Some(log) = self.stats_sink.as_mut() {
            log.record(&self.stats);
        }
    }

    /// Acts on what the automatic switch decided.
    pub(super) fn apply_switch_event(&mut self, event: SwitchEvent) {
        match event {
            SwitchEvent::ToCompressed { fps_before } => {
                eprintln!("[auto] USB too slow for the uncompressed mode ({fps_before:.0} fps): switching to a compressed mode");
                self.settings.video_mode = VideoModePref::Compressed;
            }
            SwitchEvent::Judged {
                before,
                now,
                helped,
            } => {
                eprintln!("[auto] compressed mode: {now:.0} fps (was {before:.0} fps)");
                if helped {
                    return;
                }
                eprintln!("[auto] no clear improvement: going back to automatic mode");
                self.settings.video_mode = VideoModePref::Auto;
            }
        }
        self.save_settings();
        self.select_video(self.video_index);
    }
}
