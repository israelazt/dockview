//! What the statistics say: the rows to show for a measurement, the sentence
//! for each finding and the small text helpers they need. No drawing here; the
//! card that shows them is `stats.rs`.

use dockview_domain::diagnostics::Finding;
use dockview_domain::i18n::{tr, Key, Lang};
use dockview_domain::stats::Stats;
use dockview_domain::theme::Theme;

use super::{TextStyle, Ui};

/// One titled group of label/value rows.
#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    pub title: &'static str,
    pub rows: Vec<(&'static str, String)>,
}

/// Builds the rows to show, already formatted for `lang`.
pub fn sections(stats: &Stats, lang: Lang) -> Vec<Section> {
    let measuring = || tr(lang, Key::Measuring).to_string();
    let rate = |fps: f32| {
        if stats.measured {
            format!("{fps:.0} fps")
        } else {
            measuring()
        }
    };

    let mut video = vec![(
        tr(lang, Key::DeviceMode),
        stats.device_mode.clone().unwrap_or_else(measuring),
    )];
    if let Some(mbps) = stats.mode_mbps {
        video.push((tr(lang, Key::Bandwidth), format!("{mbps:.0} Mbps")));
    }
    video.push((tr(lang, Key::Received), rate(stats.received_fps)));
    video.push((tr(lang, Key::Shown), rate(stats.shown_fps)));
    video.push((
        tr(lang, Key::Dropped),
        if stats.measured {
            format!("{:.1} /s", stats.dropped_per_sec)
        } else {
            measuring()
        },
    ));

    let mut performance = vec![(
        tr(lang, Key::AppLatency),
        stats
            .latency_ms
            .map_or_else(measuring, |ms| format!("{ms:.1} ms")),
    )];
    performance.push((
        tr(lang, Key::Cpu),
        if stats.measured {
            // Small values keep a decimal so an idle app does not read "0%".
            let core = if stats.cpu_core_percent < 10.0 {
                format!("{:.1}%", stats.cpu_core_percent)
            } else {
                format!("{:.0}%", stats.cpu_core_percent)
            };
            format!("{core} ({:.1}% total)", stats.cpu_total_percent)
        } else {
            measuring()
        },
    ));
    performance.push((
        tr(lang, Key::GpuPerFrame),
        stats
            .gpu_ms
            .map_or_else(measuring, |ms| format!("{ms:.2} ms")),
    ));
    performance.push((tr(lang, Key::Memory), format!("{:.0} MB", stats.memory_mb)));

    let mut extra = Vec::new();
    if let Some(ms) = stats.audio_buffer_ms {
        extra.push((tr(lang, Key::AudioBuffer), format!("{ms} ms")));
    }
    if let Some(hz) = stats.monitor_hz {
        extra.push((tr(lang, Key::Monitor), format!("{hz:.0} Hz")));
    }
    extra.push((
        tr(lang, Key::WindowSize),
        format!("{}×{}", stats.window.0, stats.window.1),
    ));

    vec![
        Section {
            title: tr(lang, Key::Video),
            rows: video,
        },
        Section {
            title: tr(lang, Key::Performance),
            rows: performance,
        },
        Section {
            title: tr(lang, Key::Display),
            rows: extra,
        },
    ]
}

/// The translated sentence for a finding.
pub fn finding_text(finding: Finding, lang: Lang) -> &'static str {
    tr(
        lang,
        match finding {
            Finding::Healthy => Key::FindHealthy,
            Finding::NoVideo => Key::FindNoVideo,
            Finding::VideoLost => Key::FindVideoLost,
            Finding::NeedsUsb3 => Key::FindNeedsUsb3,
            Finding::CaptureBelowMode => Key::FindCaptureLow,
            Finding::DroppingFrames => Key::FindDropping,
            Finding::HighCpu => Key::FindHighCpu,
            Finding::SlowGpu => Key::FindSlowGpu,
            Finding::AudioLost => Key::FindAudioLost,
            Finding::SwitchedToCompressed => Key::FindSwitched,
        },
    )
}

/// Greedy word wrap to `max_w` logical pixels.
pub fn wrap_text(ui: &mut Ui, text: &str, style: TextStyle, max_w: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let candidate = if current.is_empty() {
            word.to_owned()
        } else {
            format!("{current} {word}")
        };
        if current.is_empty() || ui.measure(&candidate, style).0 <= max_w {
            current = candidate;
        } else {
            lines.push(std::mem::take(&mut current));
            current = word.to_owned();
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

/// Truncates a value that would overflow its column.
pub(super) fn fit(ui: &mut Ui, text: &str, style: TextStyle, max_w: f32) -> String {
    if ui.measure(text, style).0 <= max_w {
        return text.to_owned();
    }
    let chars: Vec<char> = text.chars().collect();
    for keep in (1..chars.len()).rev() {
        let candidate = chars[..keep].iter().collect::<String>() + "…";
        if ui.measure(&candidate, style).0 <= max_w {
            return candidate;
        }
    }
    "…".to_owned()
}

pub(crate) fn tone_color(finding: Finding, theme: &Theme) -> dockview_domain::theme::Color {
    match finding {
        Finding::Healthy => theme.live,
        Finding::NoVideo | Finding::VideoLost | Finding::AudioLost => theme.error,
        Finding::NeedsUsb3
        | Finding::CaptureBelowMode
        | Finding::DroppingFrames
        | Finding::HighCpu
        | Finding::SlowGpu
        | Finding::SwitchedToCompressed => theme.warn,
    }
}
