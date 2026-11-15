//! The row of readouts over the picture in the viewer: where it comes from,
//! how long it takes, what it costs and whether the replay is armed. It draws
//! in the content's coordinates and has one button, the one that opens the
//! pipeline health.

use dockview_domain::design::RADIUS_CONTROL;
use dockview_domain::i18n::Lang;
use dockview_domain::stats::Stats;
use dockview_domain::theme::Theme;
use dockview_domain::view::{Action, Model};

#[cfg(test)]
use super::hud_cards::source_name;
use super::hud_cards::{cards, Card, Piece};
use super::input::Pointer;
use super::text::Weight;
use super::{Rect, TextStyle, Ui};

const ICON_HEALTH: char = '\u{e038}';
const ICON_CHEVRON: char = '\u{e06d}';

const MARGIN: f32 = 16.0;
const TOP: f32 = 12.0;
const CARD_H: f32 = 52.0;
const GAP: f32 = 12.0;
const PAD: f32 = 18.0;
const BUTTON: f32 = 52.0;
/// Space between two pieces of a card.
const PIECE_GAP: f32 = 14.0;
/// Space on each side of a divider.
const DIVIDER_GAP: f32 = 16.0;
/// Room the row takes from the top of the content, so what else anchors to
/// the top (notices, the recording mark) can sit below it.
pub const HEIGHT: f32 = TOP + CARD_H + GAP;
const FADE_SECS: f32 = 0.18;
const SLIDE: f32 = 6.0;

const VALUE: TextStyle = TextStyle::new(14.0, Weight::SemiBold);
const LABEL: TextStyle = TextStyle::new(11.0, Weight::Medium);
const MAX_NAME_W: f32 = 210.0;

/// `text` cut to `max` pixels with an ellipsis.
fn ellipsize(ui: &mut Ui, text: &str, style: TextStyle, max: f32) -> String {
    if ui.measure(text, style).0 <= max {
        return text.to_string();
    }
    let mut kept: Vec<char> = text.chars().collect();
    while !kept.is_empty() {
        kept.pop();
        let candidate: String = kept.iter().collect::<String>() + "…";
        if ui.measure(&candidate, style).0 <= max {
            return candidate;
        }
    }
    "…".to_string()
}

/// How many of `widths` fit in `room`, each separated by `GAP`.
fn how_many_fit(widths: &[f32], room: f32) -> usize {
    let mut used = 0.0;
    let mut count = 0;
    for (i, w) in widths.iter().enumerate() {
        let next = used + w + if i > 0 { GAP } else { 0.0 };
        if next > room {
            break;
        }
        used = next;
        count += 1;
    }
    count
}

#[derive(Debug, Default)]
pub struct Hud {
    actions: Vec<Action>,
    /// 0 (hidden) to 1 (shown).
    progress: f32,
    hover: bool,
    pressed: bool,
    /// Where the source card was drawn last, and what the pointer did on it.
    source: Option<Rect>,
    source_hover: bool,
    source_pressed: bool,
    source_clicked: bool,
}

impl Hud {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn take_actions(&mut self) -> Vec<Action> {
        std::mem::take(&mut self.actions)
    }

    /// Whether the pointer is over the health button or the source card.
    pub fn wants_pointer(&self) -> bool {
        (self.hover || self.source_hover) && self.progress > 0.0
    }

    /// Where the source card is, when the row is shown (the list opens under it).
    pub fn source_card(&self) -> Option<Rect> {
        self.source.filter(|_| self.progress > 0.0)
    }

    /// Whether the source card was clicked since the last call.
    pub fn take_source_click(&mut self) -> bool {
        std::mem::take(&mut self.source_clicked)
    }

    /// Whether the row is moving (so the window keeps drawing frames).
    pub fn is_animating(&self, show: bool) -> bool {
        let target = if show { 1.0 } else { 0.0 };
        (self.progress - target).abs() > f32::EPSILON
    }

    /// Draws the row when `show`, fading it in and out. Returns the room it
    /// takes at the top of the content (0 while it is hidden).
    #[allow(clippy::too_many_arguments)]
    pub fn frame(
        &mut self,
        ui: &mut Ui,
        model: &Model,
        stats: &Stats,
        pointer: &Pointer,
        show: bool,
        dt: f32,
        animations: bool,
    ) -> f32 {
        let target = if show { 1.0 } else { 0.0 };
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
            self.hover = false;
            self.pressed = false;
            self.source = None;
            self.source_hover = false;
            self.source_pressed = false;
            return 0.0;
        }
        let eased = 1.0 - (1.0 - self.progress).powi(3);
        let theme = &model.theme;
        let (width, _) = ui.logical_size();

        // The health button, at the right end.
        let button = Rect::new(width - MARGIN - BUTTON, TOP, BUTTON, CARD_H);
        self.hover = pointer.pos.is_some_and(|(x, y)| button.contains(x, y));
        if show {
            if pointer.just_pressed {
                self.pressed = self.hover;
            }
            if pointer.just_released {
                if self.pressed && self.hover {
                    self.actions.push(Action::ToggleStats);
                }
                self.pressed = false;
            }
        }

        ui.set_opacity(eased);
        ui.set_offset(0.0, -(1.0 - eased) * SLIDE);
        let all = cards(model, stats);
        let widths: Vec<f32> = all.iter().map(|c| card_width(ui, c, model.lang)).collect();
        let room = button.x - GAP - MARGIN;
        let shown = how_many_fit(&widths, room).max(1);
        let mut x = MARGIN;
        self.source = None;
        for (i, (card, w)) in all.iter().zip(&widths).take(shown).enumerate() {
            let w = w.min((room - x + MARGIN).max(40.0));
            let rect = Rect::new(x, TOP, w, CARD_H);
            draw_card(ui, theme, card, rect);
            if i == 0 {
                // The first card is the source: it opens the list of sources.
                self.source = Some(rect);
                self.source_hover = pointer.pos.is_some_and(|(px, py)| rect.contains(px, py));
                if self.source_hover {
                    ui.rect(rect, RADIUS_CONTROL + 2.0, theme.hover);
                }
            }
            x += w + GAP;
        }
        if show {
            if pointer.just_pressed {
                self.source_pressed = self.source_hover;
            }
            if pointer.just_released {
                if self.source_pressed && self.source_hover {
                    self.source_clicked = true;
                }
                self.source_pressed = false;
            }
        }
        self.draw_button(ui, theme, button, model.stats_open);
        ui.set_offset(0.0, 0.0);
        ui.set_opacity(1.0);
        HEIGHT
    }

    fn draw_button(&self, ui: &mut Ui, theme: &Theme, r: Rect, on: bool) {
        let (fill, border, icon) = if on {
            (
                theme.brand.with_alpha(0.14),
                theme.brand.with_alpha(0.4),
                theme.brand,
            )
        } else if self.hover {
            (theme.segment_active, theme.border, theme.text)
        } else {
            (theme.surface, theme.border, theme.text_dim)
        };
        ui.panel(r, RADIUS_CONTROL + 2.0, fill, border, 1.0);
        let (cx, cy) = r.center();
        ui.icon(ICON_HEALTH, cx, cy, 17.0, icon);
    }
}

fn piece_width(ui: &mut Ui, p: &Piece) -> f32 {
    match p {
        Piece::Dot(_) => 6.0 + 10.0,
        Piece::Stack(top, bottom, _) => {
            let top = clip_name(ui, top);
            ui.measure(&top, VALUE).0.max(ui.measure(bottom, LABEL).0)
        }
        Piece::Line(text, _) => ui.measure(text, LABEL).0 + 16.0,
        Piece::Divider => 1.0 + DIVIDER_GAP * 2.0,
        Piece::Chevron => 8.0 + 14.0,
    }
}

/// Source names can be long; the card never grows past `MAX_NAME_W` for it.
fn clip_name(ui: &mut Ui, text: &str) -> String {
    ellipsize(ui, text, VALUE, MAX_NAME_W)
}

fn card_width(ui: &mut Ui, card: &Card, _lang: Lang) -> f32 {
    // The same walk `draw_card` makes: a gap goes between two texts that are
    // not separated by a divider.
    let mut width = PAD * 2.0;
    let mut first_text = true;
    for piece in &card.pieces {
        match piece {
            Piece::Stack(..) | Piece::Line(..) => {
                if !first_text {
                    width += PIECE_GAP;
                }
                first_text = false;
            }
            Piece::Divider => first_text = true,
            Piece::Dot(_) | Piece::Chevron => {}
        }
        width += piece_width(ui, piece);
    }
    width
}

fn draw_card(ui: &mut Ui, theme: &Theme, card: &Card, r: Rect) {
    ui.shadow(r, RADIUS_CONTROL + 2.0, 10.0, theme.shadow);
    ui.panel(r, RADIUS_CONTROL + 2.0, theme.surface, theme.border, 1.0);
    let mid = r.y + r.h / 2.0;
    let mut x = r.x + PAD;
    let mut first_text = true;
    for piece in &card.pieces {
        match piece {
            Piece::Dot(color) => {
                ui.rect(Rect::new(x, mid - 3.0, 6.0, 6.0), 3.0, *color);
                x += 16.0;
            }
            Piece::Stack(top, bottom, color) => {
                if !first_text {
                    x += PIECE_GAP;
                }
                first_text = false;
                let top = clip_name(ui, top);
                let (w, th) = ui.measure(&top, VALUE);
                let (bw, bh) = ui.measure(bottom, LABEL);
                let gap = 1.0;
                let y = mid - (th + gap + bh) / 2.0;
                ui.text(&top, x, y, VALUE, *color);
                ui.text(bottom, x, y + th + gap, LABEL, theme.text_dim);
                x += w.max(bw);
            }
            Piece::Line(text, color) => {
                if !first_text {
                    x += PIECE_GAP;
                }
                first_text = false;
                let (w, h) = ui.measure(text, LABEL);
                let chip = Rect::new(x, mid - (h + 8.0) / 2.0, w + 16.0, h + 8.0);
                ui.panel(
                    chip,
                    chip.h / 2.0,
                    color.with_alpha(0.14),
                    color.with_alpha(0.35),
                    1.0,
                );
                ui.text(text, chip.x + 8.0, chip.y + 4.0, LABEL, *color);
                x += chip.w;
            }
            Piece::Chevron => {
                ui.icon(ICON_CHEVRON, x + 8.0 + 7.0, mid, 14.0, theme.text_dim);
                x += 8.0 + 14.0;
            }
            Piece::Divider => {
                x += DIVIDER_GAP;
                ui.rect(Rect::new(x, mid - 10.0, 1.0, 20.0), 0.0, theme.chrome_line);
                x += 1.0 + DIVIDER_GAP;
                first_text = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_model::model;
    use dockview_domain::view::{CurrentSource, ReplayState};

    fn measured() -> Stats {
        Stats {
            measured: true,
            shown_fps: 60.0,
            latency_ms: Some(1.2),
            cpu_core_percent: 1.2,
            memory_mb: 52.0,
            gpu_ms: Some(0.08),
            device_mode: Some("NV12 · 1920×1080 · 60fps".into()),
            ..Stats::default()
        }
    }

    fn frame(h: &mut Hud, m: &Model, s: &Stats, width: u32, p: &Pointer, show: bool) -> (Ui, f32) {
        let mut ui = Ui::new();
        ui.begin((width, 300), 1.0);
        let used = h.frame(&mut ui, m, s, p, show, 1.0, false);
        (ui, used)
    }

    fn texts_of(cards: &[Card]) -> Vec<String> {
        cards
            .iter()
            .flat_map(|c| c.pieces.iter())
            .flat_map(|p| match p {
                Piece::Stack(a, b, _) => vec![a.clone(), b.clone()],
                Piece::Line(a, _) => vec![a.clone()],
                _ => vec![],
            })
            .collect()
    }

    #[test]
    fn the_cards_say_what_the_app_measures() {
        let mut m = model();
        m.lang = Lang::En;
        m.resolution = "1080p · 60 fps".into();
        m.video_devices = vec!["HDMI 1: Elgato 4K60 Pro".into()];
        m.video_selected = Some(0);
        m.replay.state = ReplayState::Recording;
        m.replay.keep_seconds = 30;
        m.replay.hotkey = "Ctrl+Alt+S".into();
        m.nav.drops = 2;
        let texts = texts_of(&cards(&m, &measured()));
        for want in [
            "HDMI 1: Elgato 4K60 Pro",
            "1080p · 60 fps · NV12",
            "1.2 ms",
            "ULTRA LOW",
            "60 fps",
            "2",
            "52 MB",
            "1.2%",
            "0.08 ms",
            "REPLAY ARMED",
            "30s · Ctrl+Alt+S",
        ] {
            assert!(texts.iter().any(|t| t == want), "{want} in {texts:?}");
        }
    }

    #[test]
    fn before_the_first_measurement_it_shows_dots_not_zeros() {
        let m = model();
        let texts = texts_of(&cards(&m, &Stats::default()));
        assert!(texts.iter().any(|t| t == "…"));
        assert!(!texts.iter().any(|t| t == "0 fps"));
    }

    #[test]
    fn the_source_is_named_for_a_screen_and_a_window_too() {
        let mut m = model();
        m.sources.monitors = vec!["Monitor 1".into(), "Monitor 2".into()];
        m.sources.windows = vec!["Game".into()];
        m.sources.current = CurrentSource::Monitor(1);
        assert_eq!(source_name(&m), "Monitor 2");
        m.sources.current = CurrentSource::Window(Some(0));
        assert_eq!(source_name(&m), "Game");
        m.sources.current = CurrentSource::Window(None);
        assert_eq!(source_name(&m), "—");
    }

    #[test]
    fn it_takes_room_only_while_shown() {
        let m = model();
        let mut hud = Hud::new();
        let (_, hidden) = frame(&mut hud, &m, &measured(), 1280, &Pointer::default(), false);
        assert_eq!(hidden, 0.0);
        let (ui, shown) = frame(&mut hud, &m, &measured(), 1280, &Pointer::default(), true);
        assert_eq!(shown, HEIGHT);
        assert!(!ui.vertices().is_empty());
        assert!(!hud.is_animating(true));
        assert!(hud.is_animating(false), "it would fade out");
    }

    #[test]
    fn the_health_button_asks_for_the_statistics() {
        let m = model();
        let mut hud = Hud::new();
        let at = (1280.0 - MARGIN - BUTTON / 2.0, TOP + CARD_H / 2.0);
        let mut p = Pointer {
            pos: Some(at),
            ..Pointer::default()
        };
        frame(&mut hud, &m, &measured(), 1280, &p, true);
        assert!(hud.wants_pointer());
        p.press();
        frame(&mut hud, &m, &measured(), 1280, &p, true);
        p.end_frame();
        p.release();
        frame(&mut hud, &m, &measured(), 1280, &p, true);
        assert_eq!(hud.take_actions(), [Action::ToggleStats]);
        // A click elsewhere on the row does nothing.
        let mut p = Pointer {
            pos: Some((300.0, 20.0)),
            ..Pointer::default()
        };
        p.press();
        frame(&mut hud, &m, &measured(), 1280, &p, true);
        p.end_frame();
        p.release();
        frame(&mut hud, &m, &measured(), 1280, &p, true);
        assert!(hud.take_actions().is_empty());
    }

    #[test]
    fn a_hidden_row_takes_no_clicks() {
        let m = model();
        let mut hud = Hud::new();
        let at = (1280.0 - MARGIN - BUTTON / 2.0, TOP + CARD_H / 2.0);
        let mut p = Pointer {
            pos: Some(at),
            ..Pointer::default()
        };
        p.press();
        frame(&mut hud, &m, &measured(), 1280, &p, false);
        p.end_frame();
        p.release();
        frame(&mut hud, &m, &measured(), 1280, &p, false);
        assert!(hud.take_actions().is_empty());
        assert!(!hud.wants_pointer());
    }

    #[test]
    fn a_narrow_window_drops_the_last_cards() {
        let m = model();
        let wide = frame(
            &mut Hud::new(),
            &m,
            &measured(),
            1600,
            &Pointer::default(),
            true,
        )
        .0;
        let narrow = frame(
            &mut Hud::new(),
            &m,
            &measured(),
            520,
            &Pointer::default(),
            true,
        )
        .0;
        assert!(narrow.vertices().len() < wide.vertices().len());
        assert_eq!(how_many_fit(&[100.0, 100.0, 100.0], 215.0), 2);
        assert_eq!(how_many_fit(&[100.0], 99.0), 0);
    }

    #[test]
    fn long_names_are_cut_with_an_ellipsis() {
        let mut ui = Ui::new();
        ui.begin((800, 200), 1.0);
        let cut = ellipsize(
            &mut ui,
            &"A very long capture device name ".repeat(4),
            VALUE,
            120.0,
        );
        assert!(cut.ends_with('…'));
        assert!(ui.measure(&cut, VALUE).0 <= 120.0);
        assert_eq!(ellipsize(&mut ui, "Short", VALUE, 120.0), "Short");
    }

    #[test]
    fn it_survives_a_tiny_window() {
        let m = model();
        let _ = frame(
            &mut Hud::new(),
            &m,
            &measured(),
            60,
            &Pointer::default(),
            true,
        );
    }

    #[test]
    fn the_source_card_opens_the_list_when_clicked() {
        let m = model();
        let mut hud = Hud::new();
        assert!(hud.source_card().is_none(), "nothing before it is drawn");
        frame(&mut hud, &m, &measured(), 1280, &Pointer::default(), true);
        let card = hud.source_card().expect("the first card");
        assert_eq!((card.x, card.y), (MARGIN, TOP));
        let mut p = Pointer {
            pos: Some(card.center()),
            ..Pointer::default()
        };
        frame(&mut hud, &m, &measured(), 1280, &p, true);
        assert!(hud.wants_pointer(), "the hand cursor over the card");
        p.press();
        frame(&mut hud, &m, &measured(), 1280, &p, true);
        p.end_frame();
        p.release();
        frame(&mut hud, &m, &measured(), 1280, &p, true);
        assert!(hud.take_source_click());
        assert!(!hud.take_source_click(), "once");
        // A click on the second card is not the source card's.
        let other = Pointer {
            pos: Some((card.x + card.w + GAP + 30.0, TOP + 10.0)),
            ..Pointer::default()
        };
        let mut p = other;
        frame(&mut hud, &m, &measured(), 1280, &p, true);
        p.press();
        frame(&mut hud, &m, &measured(), 1280, &p, true);
        p.end_frame();
        p.release();
        frame(&mut hud, &m, &measured(), 1280, &p, true);
        assert!(!hud.take_source_click());
    }

    #[test]
    fn the_source_card_is_gone_while_the_row_is_hidden() {
        let m = model();
        let mut hud = Hud::new();
        frame(&mut hud, &m, &measured(), 1280, &Pointer::default(), true);
        assert!(hud.source_card().is_some());
        frame(&mut hud, &m, &measured(), 1280, &Pointer::default(), false);
        assert!(hud.source_card().is_none());
    }
}
