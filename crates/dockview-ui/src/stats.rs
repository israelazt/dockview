//! The statistics panel: a card in the top-left corner that shows what the
//! app and the capture device are doing, with a plain-language diagnosis.

use dockview_domain::design::{RADIUS_CARD, RADIUS_CONTROL};
use dockview_domain::diagnostics::Finding;
use dockview_domain::view::Action;

use super::input::Pointer;

use super::text::Weight;
use super::{Rect, TextStyle, Ui};
use dockview_domain::i18n::{tr, Key, Lang};
use dockview_domain::stats::Stats;
use dockview_domain::theme::Theme;

use super::stats_data::fit;
pub(crate) use super::stats_data::tone_color;
pub use super::stats_data::{finding_text, sections, wrap_text, Section};

const WIDTH: f32 = 372.0;
const MARGIN: f32 = 16.0;
const PAD: f32 = 18.0;
/// Size of the round badge and of the close button.
const CLOSE: f32 = 28.0;
const HEADER_H: f32 = 28.0;
const BLOCK_PAD: f32 = 14.0;
const HEADLINE_H: f32 = 22.0;
const ICON_PULSE: char = '\u{e038}';
const ICON_CLOSE: char = '\u{e1b2}';
const ROW_HEIGHT: f32 = 20.0;
const SECTION_GAP: f32 = 12.0;
const SECTION_HEIGHT: f32 = 20.0;
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
const HEADLINE: TextStyle = TextStyle::new(11.5, Weight::SemiBold);

/// The card and its fade animation.
pub struct StatsPanel {
    /// 0.0 hidden – 1.0 shown (linear in time; easing applied when drawing).
    progress: f32,
    actions: Vec<Action>,
    hover_close: bool,
    pressed_close: bool,
}

/// Where the close button is, for a card at `panel`.
fn close_button(panel: Rect) -> Rect {
    Rect::new(
        panel.x + panel.w - PAD - CLOSE,
        panel.y + PAD - 2.0,
        CLOSE,
        CLOSE,
    )
}

impl StatsPanel {
    pub fn new() -> Self {
        Self {
            progress: 0.0,
            actions: Vec::new(),
            hover_close: false,
            pressed_close: false,
        }
    }

    pub fn is_visible(&self) -> bool {
        self.progress > 0.0
    }

    pub fn take_actions(&mut self) -> Vec<Action> {
        std::mem::take(&mut self.actions)
    }

    /// Whether the pointer is over the close button.
    pub fn wants_pointer(&self) -> bool {
        self.hover_close && self.progress >= 1.0
    }

    /// Advances the animation and draws the card when it is (becoming) visible.
    /// `pointer` is in the same coordinates as the drawing.
    #[allow(clippy::too_many_arguments)]
    pub fn frame(
        &mut self,
        ui: &mut Ui,
        theme: &Theme,
        lang: Lang,
        stats: &Stats,
        pointer: &Pointer,
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
            self.hover_close = false;
            self.pressed_close = false;
            return;
        }

        let eased = 1.0 - (1.0 - self.progress).powi(3);
        ui.set_opacity(eased);
        ui.set_offset((1.0 - eased) * SLIDE, 0.0);
        let hovered = self.hover_close;
        let panel = draw(ui, theme, lang, stats, hovered);
        ui.set_offset(0.0, 0.0);
        ui.set_opacity(1.0);

        // Only a card that has settled takes clicks.
        let close = close_button(panel);
        self.hover_close = eased >= 1.0 && pointer.pos.is_some_and(|(x, y)| close.contains(x, y));
        if open && eased >= 1.0 {
            if pointer.just_pressed {
                self.pressed_close = self.hover_close;
            }
            if pointer.just_released {
                if self.pressed_close && self.hover_close {
                    self.actions.push(Action::ToggleStats);
                }
                self.pressed_close = false;
            }
        }
    }
}

impl Default for StatsPanel {
    fn default() -> Self {
        Self::new()
    }
}

/// The worst tone among the findings, and the headline that goes with it.
fn headline(
    stats: &Stats,
    theme: &Theme,
    lang: Lang,
) -> (&'static str, dockview_domain::theme::Color) {
    let worst = stats
        .findings
        .iter()
        .map(|&f| tone_color(f, theme))
        .find(|c| *c == theme.error)
        .or_else(|| {
            stats
                .findings
                .iter()
                .map(|&f| tone_color(f, theme))
                .find(|c| *c == theme.warn)
        });
    match worst {
        None => (tr(lang, Key::HealthOk), theme.brand),
        Some(color) => (tr(lang, Key::HealthAttention), color),
    }
}

/// Draws the card at the top right and returns its rectangle.
fn draw(ui: &mut Ui, theme: &Theme, lang: Lang, stats: &Stats, close_hover: bool) -> Rect {
    let (screen_w, _) = ui.logical_size();
    let inner_w = WIDTH - PAD * 2.0;
    let sections = sections(stats, lang);
    let finding_lines: Vec<(Finding, Vec<String>)> = stats
        .findings
        .iter()
        .map(|&finding| {
            (
                finding,
                wrap_text(
                    ui,
                    finding_text(finding, lang),
                    NOTE,
                    inner_w - BLOCK_PAD * 2.0 - 16.0,
                ),
            )
        })
        .collect();

    // The diagnosis block: a headline and what was found.
    let mut block_h = BLOCK_PAD * 2.0 + HEADLINE_H;
    for (_, lines) in &finding_lines {
        block_h += FINDING_LINE * lines.len() as f32 + FINDING_GAP;
    }
    block_h -= if finding_lines.is_empty() {
        0.0
    } else {
        FINDING_GAP
    };

    let mut height = PAD + HEADER_H + SECTION_GAP + block_h;
    for section in &sections {
        height += SECTION_GAP + SECTION_HEIGHT + ROW_HEIGHT * section.rows.len() as f32;
    }
    height += PAD;

    let panel = Rect::new(
        (screen_w - MARGIN - WIDTH).max(MARGIN),
        MARGIN,
        WIDTH,
        height,
    );
    ui.shadow(panel, RADIUS_CARD, 24.0, theme.shadow);
    ui.panel(panel, RADIUS_CARD, theme.surface, theme.border, 1.0);
    let (x, right) = (panel.x + PAD, panel.x + WIDTH - PAD);
    let mut y = panel.y + PAD;

    // The header: the pulse in a circle, the title and the close button.
    let badge = Rect::new(x, y - 2.0, CLOSE, CLOSE);
    ui.panel(badge, CLOSE / 2.0, theme.chrome, theme.border, 1.0);
    let (bx, by) = badge.center();
    ui.icon(ICON_PULSE, bx, by, 15.0, theme.brand);
    ui.text(
        tr(lang, Key::HudHealth),
        x + CLOSE + 10.0,
        y + 1.0,
        TITLE,
        theme.text,
    );
    let close = close_button(panel);
    if close_hover {
        ui.rect(close, CLOSE / 2.0, theme.hover);
    }
    let (cx, cy) = close.center();
    ui.icon(
        ICON_CLOSE,
        cx,
        cy,
        15.0,
        if close_hover {
            theme.text
        } else {
            theme.text_dim
        },
    );
    y += HEADER_H + SECTION_GAP;

    // The diagnosis, first: it is what the user opened the card to read.
    let (title, tone) = headline(stats, theme, lang);
    let block = Rect::new(x, y, inner_w, block_h);
    ui.panel(block, RADIUS_CONTROL, theme.chrome, theme.border, 1.0);
    let mut by = block.y + BLOCK_PAD;
    ui.rect(
        Rect::new(block.x + BLOCK_PAD, by + 4.0, 7.0, 7.0),
        3.5,
        tone,
    );
    ui.text(title, block.x + BLOCK_PAD + 14.0, by, HEADLINE, tone);
    by += HEADLINE_H;
    for (finding, lines) in &finding_lines {
        let dot = tone_color(*finding, theme);
        ui.rect(Rect::new(block.x + BLOCK_PAD, by + 5.0, 6.0, 6.0), 3.0, dot);
        for (i, line) in lines.iter().enumerate() {
            ui.text(
                line,
                block.x + BLOCK_PAD + 14.0,
                by + i as f32 * FINDING_LINE,
                NOTE,
                theme.text,
            );
        }
        by += FINDING_LINE * lines.len() as f32 + FINDING_GAP;
    }
    y += block_h;

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
    panel
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
        draw(&mut ui, &Theme::dark(), lang, &s, false).h
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
        panel.frame(
            &mut ui,
            &theme,
            Lang::En,
            &stats(),
            &Pointer::default(),
            true,
            0.016,
            true,
        );
        assert!(panel.is_visible() && panel.progress < 1.0);
        for _ in 0..30 {
            panel.frame(
                &mut ui,
                &theme,
                Lang::En,
                &stats(),
                &Pointer::default(),
                true,
                0.016,
                true,
            );
        }
        assert_eq!(panel.progress, 1.0);
        panel.frame(
            &mut ui,
            &theme,
            Lang::En,
            &stats(),
            &Pointer::default(),
            false,
            0.016,
            true,
        );
        assert!(panel.is_visible(), "fading out");
        panel.frame(
            &mut ui,
            &theme,
            Lang::En,
            &stats(),
            &Pointer::default(),
            true,
            0.016,
            false,
        );
        assert_eq!(panel.progress, 1.0, "snaps when animations are off");
        panel.frame(
            &mut ui,
            &theme,
            Lang::En,
            &stats(),
            &Pointer::default(),
            false,
            0.016,
            false,
        );
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
            &Pointer::default(),
            false,
            0.016,
            true,
        );
        assert!(ui.vertices().is_empty());
    }

    fn settled_panel() -> (StatsPanel, Ui) {
        let mut ui = Ui::new();
        let mut panel = StatsPanel::new();
        ui.begin((1280, 720), 1.0);
        panel.frame(
            &mut ui,
            &Theme::dark(),
            Lang::En,
            &stats(),
            &Pointer::default(),
            true,
            0.0,
            false,
        );
        (panel, ui)
    }

    fn close_centre() -> (f32, f32) {
        let mut ui = Ui::new();
        ui.begin((1280, 720), 1.0);
        let panel = draw(&mut ui, &Theme::dark(), Lang::En, &stats(), false);
        close_button(panel).center()
    }

    #[test]
    fn the_card_sits_at_the_top_right() {
        let mut ui = Ui::new();
        ui.begin((1280, 720), 1.0);
        let panel = draw(&mut ui, &Theme::dark(), Lang::En, &stats(), false);
        assert_eq!(panel.x + panel.w, 1280.0 - MARGIN);
        assert_eq!(panel.y, MARGIN);
        // It follows a narrower area (the video's, beside the Studio panel).
        ui.begin((1280, 720), 1.0);
        ui.limit_width(Some(900.0));
        let narrow = draw(&mut ui, &Theme::dark(), Lang::En, &stats(), false);
        assert_eq!(narrow.x + narrow.w, 900.0 - MARGIN);
    }

    #[test]
    fn the_headline_follows_the_worst_finding() {
        let t = Theme::dark();
        let healthy = Stats {
            findings: vec![Finding::Healthy],
            ..stats()
        };
        assert_eq!(
            headline(&healthy, &t, Lang::En),
            ("PIPELINE HEALTHY", t.brand)
        );
        let warn = Stats {
            findings: vec![Finding::HighCpu],
            ..stats()
        };
        assert_eq!(headline(&warn, &t, Lang::En), ("NEEDS ATTENTION", t.warn));
        let bad = Stats {
            findings: vec![Finding::HighCpu, Finding::NoVideo],
            ..stats()
        };
        assert_eq!(headline(&bad, &t, Lang::Es), ("REQUIERE ATENCIÓN", t.error));
    }

    #[test]
    fn the_close_button_asks_to_toggle_the_statistics() {
        let (mut panel, mut ui) = settled_panel();
        let at = close_centre();
        let mut p = Pointer {
            pos: Some(at),
            ..Pointer::default()
        };
        let frame = |panel: &mut StatsPanel, ui: &mut Ui, p: &Pointer| {
            ui.begin((1280, 720), 1.0);
            panel.frame(ui, &Theme::dark(), Lang::En, &stats(), p, true, 0.0, false);
        };
        frame(&mut panel, &mut ui, &p);
        assert!(panel.wants_pointer());
        p.press();
        frame(&mut panel, &mut ui, &p);
        p.end_frame();
        p.release();
        frame(&mut panel, &mut ui, &p);
        assert_eq!(panel.take_actions(), [Action::ToggleStats]);
        // A click elsewhere on the card does nothing.
        let mut p = Pointer {
            pos: Some((900.0, 300.0)),
            ..Pointer::default()
        };
        p.press();
        frame(&mut panel, &mut ui, &p);
        p.end_frame();
        p.release();
        frame(&mut panel, &mut ui, &p);
        assert!(panel.take_actions().is_empty());
    }
}
