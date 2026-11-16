//! The Settings mode: a view of its own that takes the place of the picture
//! and the Studio. It shows what `SettingsView` holds and asks for what
//! `SettingsAction` names, and nothing else: it does not read the Studio or the
//! viewer, and they do not read it.

use std::collections::HashMap;

use dockview_domain::design::RADIUS_CARD;
use dockview_domain::i18n::{tr, Key};
use dockview_domain::settings_view::SettingsAction;
use dockview_domain::theme::{Color, Theme};
use dockview_domain::view::{Action, Model};

use crate::fit::fit_text;
use crate::input::Pointer;
use crate::motion::smooth;
use crate::options::{LANGS, PRESETS, THEMES, VIDEO_MODES};
use crate::text::Weight;
use crate::{Rect, TextStyle, Ui};

/// The widest the page gets, in logical pixels.
const MAX_WIDTH: f32 = 560.0;
const MARGIN: f32 = 28.0;
const CARD_PAD: f32 = 18.0;
const CARD_GAP: f32 = 14.0;
const LABEL_H: f32 = 20.0;
const ROW_H: f32 = 32.0;
const GAP: f32 = 14.0;
const SLIDE_RATE: f32 = 20.0;
const HOVER_RATE: f32 = 18.0;

const TITLE: TextStyle = TextStyle::new(20.0, Weight::SemiBold);
const GROUP: TextStyle = TextStyle::new(13.0, Weight::SemiBold);
const CAPTION: TextStyle = TextStyle::new(12.0, Weight::Medium);
const CHOICE: TextStyle = TextStyle::new(12.5, Weight::Medium);
const ROW_TEXT: TextStyle = TextStyle::new(13.0, Weight::Medium);
const HINT: TextStyle = TextStyle::new(11.5, Weight::Regular);

/// Where `current` is among `all`; the first when it is not there.
fn position_of<T: PartialEq>(all: &[T], current: T) -> usize {
    all.iter().position(|v| *v == current).unwrap_or(0)
}

/// The page, with what it remembers between frames: what is under the pointer,
/// how far each hover has faded in and where the indicators glide to.
pub struct SettingsPage {
    actions: Vec<Action>,
    /// What can be clicked in the frame being drawn, and in the one before.
    items: Vec<(Rect, Action)>,
    before: Vec<(Rect, Action)>,
    hover: Vec<f32>,
    slide: HashMap<&'static str, f32>,
    animations: bool,
    over_item: bool,
    dt: f32,
}

impl Default for SettingsPage {
    fn default() -> Self {
        Self {
            actions: Vec::new(),
            items: Vec::new(),
            before: Vec::new(),
            hover: Vec::new(),
            slide: HashMap::new(),
            animations: true,
            over_item: false,
            dt: 0.0,
        }
    }
}

impl SettingsPage {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_animations(&mut self, on: bool) {
        self.animations = on;
    }

    pub fn take_actions(&mut self) -> Vec<Action> {
        std::mem::take(&mut self.actions)
    }

    /// Whether the pointer is over a choice (hand cursor).
    pub fn wants_pointer(&self) -> bool {
        self.over_item
    }

    /// Draws the page in `area` and handles the pointer, which is in the same
    /// coordinates as `area`.
    pub fn frame(&mut self, ui: &mut Ui, m: &Model, area: Rect, pointer: &Pointer, dt: f32) {
        let t = &m.theme;
        self.dt = dt;
        // What was clickable last frame is what a press means now.
        let over = pointer
            .pos
            .and_then(|(x, y)| self.before.iter().position(|(r, _)| r.contains(x, y)));
        self.over_item = over.is_some();
        if let (Some(i), true) = (over, pointer.just_pressed) {
            self.actions.push(self.before[i].1.clone());
        }
        self.hover
            .resize(self.before.len().max(self.hover.len()), 0.0);
        for (i, amount) in self.hover.iter_mut().enumerate() {
            let target = if over == Some(i) { 1.0 } else { 0.0 };
            *amount = if self.animations {
                smooth(*amount, target, dt, HOVER_RATE)
            } else {
                target
            };
        }
        self.items.clear();

        ui.rect(area, 0.0, t.backdrop);
        let width = (area.w - MARGIN * 2.0).clamp(0.0, MAX_WIDTH);
        let x = area.x + (area.w - width) / 2.0;
        let mut y = area.y + MARGIN;
        ui.text(tr(m.lang, Key::NavSettings), x, y, TITLE, t.text);
        y += ui.measure("Ag", TITLE).1 + CARD_GAP + 4.0;

        y += self.picture_card(ui, m, x, y, width) + CARD_GAP;
        y += self.look_card(ui, m, x, y, width) + CARD_GAP;
        self.app_card(ui, m, x, y, width);

        std::mem::swap(&mut self.items, &mut self.before);
    }

    /// Registers a click area and returns its hover amount.
    fn item(&mut self, rect: Rect, action: Action) -> f32 {
        let index = self.items.len();
        self.items.push((rect, action));
        self.hover.get(index).copied().unwrap_or(0.0)
    }

    fn card(&self, ui: &mut Ui, t: &Theme, title: &str, r: Rect) {
        ui.panel(r, RADIUS_CARD, t.surface_solid, t.border, 1.0);
        ui.text(title, r.x + CARD_PAD, r.y + CARD_PAD - 2.0, GROUP, t.text);
    }

    /// Height of a card with `controls` labelled rows of choices.
    fn card_height(controls: usize) -> f32 {
        let rows = controls as f32 * (LABEL_H + ROW_H) + (controls as f32 - 1.0) * GAP;
        CARD_PAD + 22.0 + rows + CARD_PAD
    }

    fn picture_card(&mut self, ui: &mut Ui, m: &Model, x: f32, y: f32, w: f32) -> f32 {
        let (t, lang) = (&m.theme, m.lang);
        let h = Self::card_height(2);
        self.card(
            ui,
            t,
            tr(lang, Key::SettingsGroupImage),
            Rect::new(x, y, w, h),
        );
        let (cx, cw) = (x + CARD_PAD, w - CARD_PAD * 2.0);
        let mut top = y + CARD_PAD + 22.0;
        let auto = tr(lang, Key::Auto);

        ui.text(tr(lang, Key::ImageQuality), cx, top, CAPTION, t.text_dim);
        top += LABEL_H;
        self.segmented(
            ui,
            t,
            "quality",
            &[
                tr(lang, Key::PresetPerformance),
                tr(lang, Key::PresetBalanced),
                tr(lang, Key::PresetHigh),
            ],
            position_of(&PRESETS, m.settings.preset),
            PRESETS
                .iter()
                .map(|p| Action::Settings(SettingsAction::SetPreset(*p)))
                .collect(),
            (cx, top, cw),
        );
        top += ROW_H + GAP;

        ui.text(tr(lang, Key::VideoMode), cx, top, CAPTION, t.text_dim);
        top += LABEL_H;
        self.segmented(
            ui,
            t,
            "video-mode",
            &[auto, tr(lang, Key::ModeCompressed)],
            position_of(&VIDEO_MODES, m.settings.video_mode),
            VIDEO_MODES
                .iter()
                .map(|v| Action::Settings(SettingsAction::SetVideoMode(*v)))
                .collect(),
            (cx, top, cw),
        );
        h
    }

    fn look_card(&mut self, ui: &mut Ui, m: &Model, x: f32, y: f32, w: f32) -> f32 {
        let (t, lang) = (&m.theme, m.lang);
        let h = Self::card_height(2);
        self.card(
            ui,
            t,
            tr(lang, Key::SettingsGroupLook),
            Rect::new(x, y, w, h),
        );
        let (cx, cw) = (x + CARD_PAD, w - CARD_PAD * 2.0);
        let mut top = y + CARD_PAD + 22.0;
        let auto = tr(lang, Key::Auto);

        ui.text(tr(lang, Key::Theme), cx, top, CAPTION, t.text_dim);
        top += LABEL_H;
        self.segmented(
            ui,
            t,
            "theme",
            &[auto, tr(lang, Key::Dark), tr(lang, Key::Light)],
            position_of(&THEMES, m.settings.theme_mode),
            THEMES
                .iter()
                .map(|v| Action::Settings(SettingsAction::SetTheme(*v)))
                .collect(),
            (cx, top, cw),
        );
        top += ROW_H + GAP;

        ui.text(tr(lang, Key::Language), cx, top, CAPTION, t.text_dim);
        top += LABEL_H;
        self.segmented(
            ui,
            t,
            "language",
            &[auto, tr(lang, Key::English), tr(lang, Key::Spanish)],
            position_of(&LANGS, m.settings.lang_mode),
            LANGS
                .iter()
                .map(|v| Action::Settings(SettingsAction::SetLang(*v)))
                .collect(),
            (cx, top, cw),
        );
        h
    }

    fn app_card(&mut self, ui: &mut Ui, m: &Model, x: f32, y: f32, w: f32) -> f32 {
        let (t, lang) = (&m.theme, m.lang);
        let h = CARD_PAD + 22.0 + 52.0 + CARD_PAD;
        self.card(
            ui,
            t,
            tr(lang, Key::SettingsGroupApp),
            Rect::new(x, y, w, h),
        );
        let (cx, cw) = (x + CARD_PAD, w - CARD_PAD * 2.0);
        let top = y + CARD_PAD + 22.0;
        self.switch_row(
            ui,
            t,
            tr(lang, Key::SettingsKeepInTray),
            tr(lang, Key::SettingsKeepInTrayHint),
            m.settings.keep_in_tray,
            Action::Settings(SettingsAction::SetKeepInTray(!m.settings.keep_in_tray)),
            (cx, top, cw),
        );
        h
    }

    /// A row of choices of which one is selected; the indicator glides from
    /// one to the next. `actions[i]` is what choosing `labels[i]` asks for.
    #[allow(clippy::too_many_arguments)]
    fn segmented(
        &mut self,
        ui: &mut Ui,
        t: &Theme,
        key: &'static str,
        labels: &[&str],
        selected: usize,
        actions: Vec<Action>,
        (x, y, w): (f32, f32, f32),
    ) {
        ui.rect(Rect::new(x, y, w, ROW_H), 11.0, t.segment_track);
        let segment_w = (w - 6.0) / labels.len().max(1) as f32;
        let line_h = ui.measure("Ag", CHOICE).1;
        let target = selected as f32;
        let current = self.slide.get(key).copied().unwrap_or(target);
        let position = if self.animations {
            smooth(current, target, self.dt, SLIDE_RATE)
        } else {
            target
        };
        self.slide.insert(key, position);
        ui.rect(
            Rect::new(
                x + 3.0 + segment_w * position,
                y + 3.0,
                segment_w,
                ROW_H - 6.0,
            ),
            8.0,
            t.segment_active,
        );
        for ((i, label), action) in labels.iter().enumerate().zip(actions) {
            let segment = Rect::new(
                x + 3.0 + segment_w * i as f32,
                y + 3.0,
                segment_w,
                ROW_H - 6.0,
            );
            let hover = self.item(segment, action);
            if i != selected && hover > 0.002 {
                ui.rect(
                    segment,
                    8.0,
                    t.hover.with_alpha((t.hover.a * hover).min(1.0)),
                );
            }
            let text = fit_text(ui, label, CHOICE, segment_w - 8.0);
            let text_w = ui.measure(&text, CHOICE).0;
            ui.text(
                &text,
                segment.x + (segment_w - text_w) / 2.0,
                y + (ROW_H - line_h) / 2.0,
                CHOICE,
                if i == selected { t.text } else { t.text_dim },
            );
        }
    }

    /// A line with a name, a hint under it and an on/off switch at the right.
    #[allow(clippy::too_many_arguments)]
    fn switch_row(
        &mut self,
        ui: &mut Ui,
        t: &Theme,
        title: &str,
        hint: &str,
        on: bool,
        action: Action,
        (x, y, w): (f32, f32, f32),
    ) {
        const SWITCH_W: f32 = 40.0;
        const SWITCH_H: f32 = 22.0;
        let row = Rect::new(x, y, w, 52.0);
        let hover = self.item(row, action);
        if hover > 0.002 {
            ui.rect(row, 10.0, t.hover.with_alpha((t.hover.a * hover).min(1.0)));
        }
        let text_w = w - SWITCH_W - 24.0;
        let title = fit_text(ui, title, ROW_TEXT, text_w);
        ui.text(&title, x + 10.0, y + 7.0, ROW_TEXT, t.text);
        let hint = fit_text(ui, hint, HINT, text_w);
        ui.text(&hint, x + 10.0, y + 28.0, HINT, t.text_dim);
        let track = Rect::new(
            x + w - SWITCH_W - 10.0,
            y + (52.0 - SWITCH_H) / 2.0,
            SWITCH_W,
            SWITCH_H,
        );
        ui.rect(
            track,
            SWITCH_H / 2.0,
            if on { t.accent } else { t.segment_track },
        );
        let knob = SWITCH_H - 6.0;
        let knob_x = if on {
            track.x + track.w - knob - 3.0
        } else {
            track.x + 3.0
        };
        ui.rect(
            Rect::new(knob_x, track.y + 3.0, knob, knob),
            knob / 2.0,
            Color::hex(0xFFFFFF, 1.0),
        );
    }
}

#[cfg(test)]
mod tests;
