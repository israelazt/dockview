//! Optional CSV log of the per-second measurements (`--log-stats`).
//!
//! Handy for judging a long session without a screenshot disturbing it, and
//! for attaching to a bug report. It only ever writes to the file the user
//! names; nothing is sent anywhere.

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;
use std::time::Instant;

use crate::ui::stats::Stats;

const HEADER: &str = "seconds,received_fps,shown_fps,skipped_per_s,latency_ms,cpu_core_percent,gpu_ms,memory_mb,audio_buffer_ms,mode\n";

pub struct StatsLog {
    out: BufWriter<File>,
    started: Instant,
}

impl StatsLog {
    /// Creates (or truncates) `path` and writes the header.
    pub fn create(path: &Path) -> io::Result<Self> {
        let mut out = BufWriter::new(File::create(path)?);
        out.write_all(HEADER.as_bytes())?;
        out.flush()?;
        Ok(Self {
            out,
            started: Instant::now(),
        })
    }

    /// Appends one line. Errors are ignored: logging must never disturb video.
    pub fn record(&mut self, stats: &Stats) {
        let line = format_row(self.started.elapsed().as_secs_f32(), stats);
        let _ = self.out.write_all(line.as_bytes());
        let _ = self.out.flush();
    }
}

/// One CSV line for `stats`, `seconds` into the session.
pub fn format_row(seconds: f32, stats: &Stats) -> String {
    let optional = |value: Option<f32>| value.map_or(String::new(), |v| format!("{v:.2}"));
    format!(
        "{seconds:.1},{:.1},{:.1},{:.1},{},{:.1},{},{:.1},{},{}\n",
        stats.received_fps,
        stats.shown_fps,
        stats.dropped_per_sec,
        optional(stats.latency_ms),
        stats.cpu_core_percent,
        optional(stats.gpu_ms),
        stats.memory_mb,
        stats
            .audio_buffer_ms
            .map_or(String::new(), |ms| ms.to_string()),
        stats.device_mode.clone().unwrap_or_default(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_has_one_field_per_header_column() {
        let stats = Stats {
            received_fps: 60.0,
            shown_fps: 59.0,
            dropped_per_sec: 1.0,
            latency_ms: Some(1.25),
            cpu_core_percent: 17.4,
            gpu_ms: Some(0.08),
            memory_mb: 75.0,
            audio_buffer_ms: Some(60),
            device_mode: Some("MJPEG · 1920×1080 · 60fps".into()),
            ..Stats::default()
        };
        let row = format_row(12.0, &stats);
        assert_eq!(
            row.trim_end().split(',').count(),
            HEADER.trim_end().split(',').count()
        );
        assert_eq!(
            row,
            "12.0,60.0,59.0,1.0,1.25,17.4,0.08,75.0,60,MJPEG · 1920×1080 · 60fps\n"
        );
    }

    #[test]
    fn unknown_values_leave_their_column_empty() {
        let row = format_row(1.0, &Stats::default());
        let fields: Vec<&str> = row.trim_end().split(',').collect();
        assert_eq!(fields.len(), 10);
        assert_eq!(fields[4], "");
        assert_eq!(fields[6], "");
        assert_eq!(fields[8], "");
        assert_eq!(fields[9], "");
    }

    #[test]
    fn the_file_gets_a_header_and_one_line_per_record() {
        let path = std::env::temp_dir().join(format!("dockview-log-{}.csv", std::process::id()));
        let mut log = StatsLog::create(&path).unwrap();
        log.record(&Stats::default());
        log.record(&Stats::default());
        drop(log);
        let text = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(text.lines().count(), 3);
        assert!(text.starts_with("seconds,received_fps"));
    }
}
