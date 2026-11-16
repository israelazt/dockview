//! Gathers the measurements the stats panel shows and acts on the automatic
//! mode switch.

use super::*;

impl Core {
    /// Refreshes the measurements once a second.
    pub(super) fn update_stats(&mut self, now: Instant) {
        let Some(Rates {
            received_fps,
            dropped_per_sec,
            shown_fps,
        }) = self.measures.telemetry.close_window(
            now,
            self.slot.published(),
            self.slot.dropped(),
            self.view.rendered,
        )
        else {
            return;
        };

        let usage = self.measures.telemetry.usage();
        let latency_ms = self
            .measures
            .telemetry
            .take_latency(self.measures.stats.latency_ms);

        let native = self.sources.native_mode();
        let video = self.sources.video_state();
        let monitor_hz = self.host.monitor_hz();
        let gpu_ms = self.host.gpu_ms();
        let audio_ok = self.sources.audio_ok();

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
            && self.sources.capture_has_compressed()
            && !self.cli.test_pattern;
        let usb_slow = findings.contains(&Finding::NeedsUsb3);
        if let Some(event) = self.measures.auto.tick(usb_slow, allowed, received_fps) {
            self.apply_switch_event(event);
        }
        let mut findings = findings;
        if self.measures.auto.is_switched() {
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

        let size = self.host.window_size();
        self.measures.stats = Stats {
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
            audio_buffer_ms: self.sources.audio_buffered_ms(),
            monitor_hz,
            window: size,
            findings,
            measured: true,
        };
        if let Some(log) = self.measures.sink.as_mut() {
            log.record(&self.measures.stats);
        }
    }

    /// Acts on what the automatic switch decided.
    pub(super) fn apply_switch_event(&mut self, event: SwitchEvent) {
        match event {
            SwitchEvent::ToCompressed { fps_before } => {
                dockview_domain::log_info!("[auto] USB too slow for the uncompressed mode ({fps_before:.0} fps): switching to a compressed mode");
                self.settings.video_mode = VideoModePref::Compressed;
            }
            SwitchEvent::Judged {
                before,
                now,
                helped,
            } => {
                dockview_domain::log_info!(
                    "[auto] compressed mode: {now:.0} fps (was {before:.0} fps)"
                );
                if helped {
                    return;
                }
                dockview_domain::log_info!(
                    "[auto] no clear improvement: going back to automatic mode"
                );
                self.settings.video_mode = VideoModePref::Auto;
            }
        }
        self.settings_changed();
        self.select_video(self.sources.video_index);
    }
}
