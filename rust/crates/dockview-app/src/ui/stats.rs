//! The statistics panel: a card in the top-left corner that shows what the
//! app and the capture device are doing, with a plain-language diagnosis.

use dockview_core::diagnostics::Finding;

use super::text::Weight;
use super::{Rect, TextStyle, Ui};
use crate::i18n::{tr, Key, Lang};
use crate::telemetry::Stats;
use crate::theme::Theme;

const WIDTH: f32 = 372.0;
const MARGIN: f32 = 16.0;
const PAD: f32 = 18.0;
const ROW_HEIGHT: f32 = 22.0;
const SECTION_GAP: f32 = 12.0;
const SECTION_HEIGHT: f32 = 22.0;
const FINDING_LINE: f32 = 18.0;
const FINDING_GAP: f32 = 6.0;
/// Time the panel takes to appear or disappear, in seconds.
const FADE_SECS: f32 = 0.18;
/// Sideways travel while appearing, in logical pixels.
const SLIDE: f32 = 10.0;

const TITLE: TextStyle = TextStyle::new(14.0, Weight::SemiBold);
const SECTION: TextStyle = TextStyle::new(12.0, Weight::Medium);
const ROW: TextStyle = TextStyle::new(13.0, Weight::Regular);
const NOTE: TextStyle = TextStyle::new(12.5, Weight::Regular);

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

/// The panel and its fade animation.
pub struct StatsPanel {
    /// 0.0 hidden – 1.0 shown (linear in time; easing applied when drawing).
    progress: f32,
}

impl StatsPanel {
    pub fn new() -> Self {
        Self { progress: 0.0 }
    }

    pub fn is_visible(&self) -> bool {
        self.progress > 0.0
    }

    /// Advances the animation and draws the panel when it is (becoming) visible.
    #[allow(clippy::too_many_arguments)]
    pub fn frame(
        &mut self,
        ui: &mut Ui,
        theme: &Theme,
        lang: Lang,
        stats: &Stats,
        open: bool,
        dt: f32,
        animations: bool,
    ) {
        let target = if open { 1.0 } else { 0.0 };
        self.progress = if animations {
            let step = dt / FADE_SECS;
            if self.progress < target {
                (self.progress + step).min(target)
            } else {
                (self.progress - step).max(target)
            }
        } else {
            target
        };
        if self.progress <= 0.0 {
            return;
        }

        let eased = 1.0 - (1.0 - self.progress).powi(3);
        ui.set_opacity(eased);
        ui.set_offset(-(1.0 - eased) * SLIDE, 0.0);
        draw(ui, theme, lang, stats);
        ui.set_offset(0.0, 0.0);
        ui.set_opacity(1.0);
    }
}

impl Default for StatsPanel {
    fn default() -> Self {
        Self::new()
    }
}

/// A line of the finding list, with its tone.
struct FindingBlock {
    finding: Finding,
    lines: Vec<String>,
}

fn draw(ui: &mut Ui, theme: &Theme, lang: Lang, stats: &Stats) -> Rect {
    let inner_w = WIDTH - PAD * 2.0;
    let sections = sections(stats, lang);
    let blocks: Vec<FindingBlock> = stats
        .findings
        .iter()
        .map(|&finding| FindingBlock {
            finding,
            lines: wrap_text(ui, finding_text(finding, lang), NOTE, inner_w - 16.0),
        })
        .collect();

    // Height: title, then each section, then the diagnosis.
    let mut height = PAD + 22.0;
    for section in &sections {
        height += SECTION_GAP + SECTION_HEIGHT + ROW_HEIGHT * section.rows.len() as f32;
    }
    height += SECTION_GAP + SECTION_HEIGHT;
    for block in &blocks {
        height += FINDING_LINE * block.lines.len() as f32 + FINDING_GAP;
    }
    height += PAD - FINDING_GAP;

    let panel = Rect::new(MARGIN, MARGIN, WIDTH, height);
    ui.shadow(panel, 18.0, 24.0, theme.shadow);
    ui.panel(panel, 18.0, theme.surface, theme.border, 1.0);

    let (x, right) = (panel.x + PAD, panel.x + WIDTH - PAD);
    let mut y = panel.y + PAD;

    // Title and the key that toggles the panel.
    ui.text(tr(lang, Key::Statistics), x, y - 1.0, TITLE, theme.text);
    let hint = TextStyle::new(12.0, Weight::Medium);
    let key_w = ui.measure("I", hint).0 + 12.0;
    let key_box = Rect::new(right - key_w, y, key_w, 20.0);
    ui.rect(key_box, 6.0, theme.segment_track);
    ui.text("I", key_box.x + 6.0, y + 1.0, hint, theme.text_dim);
    y += 22.0;

    for section in &sections {
        y += SECTION_GAP;
        ui.text(section.title, x, y + 3.0, SECTION, theme.text_dim);
        y += SECTION_HEIGHT;
        for (label, value) in &section.rows {
            ui.text(label, x, y + 2.0, ROW, theme.text_dim);
            let value = fit(ui, value, ROW.tabular(), inner_w * 0.62);
            let value_w = ui.measure(&value, ROW.tabular()).0;
            ui.text(&value, right - value_w, y + 2.0, ROW.tabular(), theme.text);
            y += ROW_HEIGHT;
        }
    }

    y += SECTION_GAP;
    ui.text(
        tr(lang, Key::Diagnosis),
        x,
        y + 3.0,
        SECTION,
        theme.text_dim,
    );
    y += SECTION_HEIGHT;
    for block in &blocks {
        let dot = tone_color(block.finding, theme);
        ui.rect(Rect::new(x, y + 5.0, 8.0, 8.0), 4.0, dot);
        for (i, line) in block.lines.iter().enumerate() {
            ui.text(
                line,
                x + 16.0,
                y + i as f32 * FINDING_LINE,
                NOTE,
                theme.text,
            );
        }
        y += FINDING_LINE * block.lines.len() as f32 + FINDING_GAP;
    }
    panel
}

/// Truncates a value that would overflow its column.
fn fit(ui: &mut Ui, text: &str, style: TextStyle, max_w: f32) -> String {
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

fn tone_color(finding: Finding, theme: &Theme) -> crate::theme::Color {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn stats() -> Stats {
        Stats {
            device_mode: Some("NV12 · 1920×1080 · 30fps".into()),
            mode_mbps: Some(746.0),
            received_fps: 29.7,
            shown_fps: 30.1,
            dropped_per_sec: 0.0,
            latency_ms: Some(4.2),
            cpu_core_percent: 7.4,
            cpu_total_percent: 0.6,
            gpu_ms: Some(0.35),
            memory_mb: 58.2,
            audio_buffer_ms: Some(62),
            monitor_hz: Some(144.0),
            window: (1280, 720),
            findings: vec![Finding::Healthy],
            measured: true,
        }
    }

    fn row<'a>(sections: &'a [Section], label: &str) -> &'a str {
        sections
            .iter()
            .flat_map(|s| s.rows.iter())
            .find(|(l, _)| *l == label)
            .map(|(_, v)| v.as_str())
            .unwrap_or_else(|| panic!("no row {label:?}"))
    }

    #[test]
    fn numbers_are_formatted_for_reading() {
        let s = sections(&stats(), Lang::En);
        assert_eq!(row(&s, "Frames received"), "30 fps");
        assert_eq!(row(&s, "Frames skipped"), "0.0 /s");
        assert_eq!(row(&s, "App latency"), "4.2 ms");
        assert_eq!(row(&s, "CPU (this app)"), "7.4% (0.6% total)");
        assert_eq!(row(&s, "GPU per frame"), "0.35 ms");
        assert_eq!(row(&s, "Memory"), "58 MB");
        assert_eq!(row(&s, "Monitor"), "144 Hz");
        assert_eq!(row(&s, "Mode data rate"), "746 Mbps");
    }

    #[test]
    fn busy_cpu_shows_whole_percentages_and_idle_keeps_a_decimal() {
        let busy = Stats {
            cpu_core_percent: 42.4,
            cpu_total_percent: 3.5,
            ..stats()
        };
        assert_eq!(
            row(&sections(&busy, Lang::En), "CPU (this app)"),
            "42% (3.5% total)"
        );
        let idle = Stats {
            cpu_core_percent: 0.3,
            cpu_total_percent: 0.0,
            ..stats()
        };
        assert_eq!(
            row(&sections(&idle, Lang::En), "CPU (this app)"),
            "0.3% (0.0% total)"
        );
    }

    #[test]
    fn rows_follow_the_language() {
        let s = sections(&stats(), Lang::Es);
        assert_eq!(row(&s, "Frames recibidos"), "30 fps");
        assert_eq!(row(&s, "Memoria"), "58 MB");
        assert_eq!(s[0].title, "Video");
        assert_eq!(s[1].title, "Rendimiento");
    }

    #[test]
    fn before_the_first_second_rates_say_measuring() {
        let fresh = Stats {
            measured: false,
            latency_ms: None,
            gpu_ms: None,
            device_mode: None,
            ..stats()
        };
        let s = sections(&fresh, Lang::En);
        assert_eq!(row(&s, "Frames received"), "Measuring…");
        assert_eq!(row(&s, "App latency"), "Measuring…");
        assert_eq!(row(&s, "Device mode"), "Measuring…");
        assert_eq!(row(&s, "CPU (this app)"), "Measuring…");
    }

    #[test]
    fn optional_rows_are_left_out_when_unknown() {
        let bare = Stats {
            audio_buffer_ms: None,
            monitor_hz: None,
            mode_mbps: None,
            ..stats()
        };
        let s = sections(&bare, Lang::En);
        let labels: Vec<&str> = s
            .iter()
            .flat_map(|s| s.rows.iter().map(|(l, _)| *l))
            .collect();
        assert!(!labels.contains(&"Audio buffer"));
        assert!(!labels.contains(&"Monitor"));
        assert!(!labels.contains(&"Mode data rate"));
        assert!(labels.contains(&"Window"));
    }

    #[test]
    fn every_finding_has_text_in_both_languages() {
        for finding in [
            Finding::Healthy,
            Finding::NoVideo,
            Finding::VideoLost,
            Finding::NeedsUsb3,
            Finding::CaptureBelowMode,
            Finding::DroppingFrames,
            Finding::HighCpu,
            Finding::SlowGpu,
            Finding::AudioLost,
            Finding::SwitchedToCompressed,
        ] {
            for lang in [Lang::En, Lang::Es] {
                assert!(
                    finding_text(finding, lang).len() > 10,
                    "{finding:?} {lang:?}"
                );
            }
        }
    }

    #[test]
    fn wrapping_keeps_every_line_within_the_width_and_loses_no_words() {
        let mut ui = Ui::new();
        ui.begin((1280, 720), 1.0);
        let text = finding_text(Finding::CaptureBelowMode, Lang::Es);
        let lines = wrap_text(&mut ui, text, NOTE, 300.0);
        assert!(lines.len() >= 2);
        for line in &lines {
            assert!(ui.measure(line, NOTE).0 <= 300.0, "{line:?} too wide");
        }
        assert_eq!(lines.join(" "), text);
    }

    #[test]
    fn a_single_overlong_word_is_kept_on_its_own_line() {
        let mut ui = Ui::new();
        ui.begin((1280, 720), 1.0);
        let lines = wrap_text(&mut ui, "supercalifragilisticexpialidocious ok", NOTE, 20.0);
        assert_eq!(lines, vec!["supercalifragilisticexpialidocious", "ok"]);
    }

    fn drawn_height(lang: Lang, findings: Vec<Finding>) -> f32 {
        let mut ui = Ui::new();
        ui.begin((1280, 720), 1.0);
        let s = Stats {
            findings,
            ..stats()
        };
        draw(&mut ui, &Theme::dark(), lang, &s).h
    }

    #[test]
    fn the_panel_grows_with_the_number_of_findings() {
        let one = drawn_height(Lang::En, vec![Finding::Healthy]);
        let three = drawn_height(
            Lang::En,
            vec![Finding::DroppingFrames, Finding::HighCpu, Finding::SlowGpu],
        );
        assert!(three > one);
    }

    #[test]
    fn the_worst_case_panel_still_fits_a_small_window() {
        let all = [
            Finding::NoVideo,
            Finding::VideoLost,
            Finding::NeedsUsb3,
            Finding::CaptureBelowMode,
            Finding::DroppingFrames,
            Finding::HighCpu,
            Finding::SlowGpu,
            Finding::AudioLost,
        ];
        for lang in [Lang::En, Lang::Es] {
            // Realistically at most a few findings appear together, but even
            // the full list should not run past a 720-pixel-high window by
            // more than the available margin.
            let h = drawn_height(lang, all[2..6].to_vec());
            assert!(h + MARGIN * 2.0 < 720.0, "{lang:?}: {h}");
        }
    }

    #[test]
    fn the_panel_fades_in_and_out_and_snaps_without_animations() {
        let mut ui = Ui::new();
        let mut panel = StatsPanel::new();
        let theme = Theme::dark();
        ui.begin((1280, 720), 1.0);
        panel.frame(&mut ui, &theme, Lang::En, &stats(), true, 0.016, true);
        assert!(panel.is_visible() && panel.progress < 1.0);
        for _ in 0..30 {
            panel.frame(&mut ui, &theme, Lang::En, &stats(), true, 0.016, true);
        }
        assert_eq!(panel.progress, 1.0);
        panel.frame(&mut ui, &theme, Lang::En, &stats(), false, 0.016, true);
        assert!(panel.is_visible(), "fading out");
        panel.frame(&mut ui, &theme, Lang::En, &stats(), true, 0.016, false);
        assert_eq!(panel.progress, 1.0, "snaps when animations are off");
        panel.frame(&mut ui, &theme, Lang::En, &stats(), false, 0.016, false);
        assert!(!panel.is_visible());
    }

    #[test]
    fn nothing_is_drawn_while_hidden() {
        let mut ui = Ui::new();
        ui.begin((1280, 720), 1.0);
        let mut panel = StatsPanel::new();
        panel.frame(
            &mut ui,
            &Theme::dark(),
            Lang::En,
            &stats(),
            false,
            0.016,
            true,
        );
        assert!(ui.vertices().is_empty());
    }
}
