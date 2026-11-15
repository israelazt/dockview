//! The shortcuts strip above the content: the keys that work, in the words of
//! the user's language, and which version is running. It only shows; it takes
//! no clicks. It draws in its own coordinates (the shell leaves room above and
//! to the left of it).

use dockview_domain::brand::{APP_NAME, VERSION};
use dockview_domain::i18n::{tr, Key, Lang};
use dockview_domain::theme::Theme;
use dockview_domain::view::{Model, StatusKind};
use dockview_domain::window_chrome::STRIP_HEIGHT;

use super::text::Weight;
use super::{Rect, TextStyle, Ui};

const SIDE: f32 = 6.0;
const BAR_H: f32 = 24.0;
const PAD: f32 = 10.0;
const GAP: f32 = 14.0;
const KEY_STYLE: TextStyle = TextStyle::new(10.5, Weight::Medium);
const TEXT_STYLE: TextStyle = TextStyle::new(11.0, Weight::Regular);
const KEY_H: f32 = 16.0;
const KEY_PAD: f32 = 6.0;
const KEY_TEXT_GAP: f32 = 5.0;
const DOT: f32 = 5.0;

/// A key (or a chord) and what it does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shortcut {
    pub keys: String,
    pub label: String,
}

/// The shortcuts that work in the window and the global ones the machine
/// registered, most useful first (the last ones go first when there is no room).
pub fn shortcuts(lang: Lang, model: &Model) -> Vec<Shortcut> {
    let word = |key| tr(lang, key).to_string();
    let mut list = vec![
        Shortcut {
            keys: "Tab".into(),
            label: word(Key::StripStudio),
        },
        Shortcut {
            keys: "F".into(),
            label: word(Key::StripFullscreen),
        },
    ];
    if !model.replay.hotkey.is_empty() {
        list.push(Shortcut {
            keys: model.replay.hotkey.clone(),
            label: word(Key::StripSaveClip),
        });
    }
    if !model.replay.record_hotkey.is_empty() {
        list.push(Shortcut {
            keys: model.replay.record_hotkey.clone(),
            label: word(Key::StripRecord),
        });
    }
    list.push(Shortcut {
        keys: "Ctrl+Alt+F1-F9".into(),
        label: word(Key::StripScenes),
    });
    list
}

/// How many of `widths` fit in `room`, each followed by `GAP` but the last.
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
pub struct ShortcutStrip;

impl ShortcutStrip {
    pub fn new() -> Self {
        Self
    }

    /// Draws the strip across the top of the area it was given and returns its
    /// height ([`STRIP_HEIGHT`]).
    pub fn frame(&mut self, ui: &mut Ui, model: &Model) -> f32 {
        let theme = &model.theme;
        let (width, _) = ui.logical_size();
        let bar = Rect::new(
            SIDE,
            (STRIP_HEIGHT - BAR_H) / 2.0,
            (width - SIDE * 2.0).max(0.0),
            BAR_H,
        );
        ui.panel(bar, 8.0, theme.chip, theme.chip_line, 1.0);
        let mid = bar.y + bar.h / 2.0;

        // The version, on the right, when it fits beside the label.
        let version = format!("{APP_NAME} v{VERSION}");
        let (version_w, version_h) = ui.measure(&version, TEXT_STYLE);
        let dot_color = match model.status {
            StatusKind::Live => theme.brand,
            StatusKind::Connecting => theme.warn,
            StatusKind::Problem => theme.error,
            StatusKind::Idle => theme.text_dim,
        };
        let label = tr(model.lang, Key::StripLabel);
        let (label_w, label_h) = ui.measure(label, TEXT_STYLE);

        let items = shortcuts(model.lang, model);
        let widths: Vec<f32> = items.iter().map(|s| self.width_of(ui, s)).collect();
        let right_w = version_w + DOT + 8.0;
        let inner = bar.w - PAD * 2.0;
        let room_for_items = |with_version: bool| {
            inner
                - label_w
                - GAP
                - if with_version {
                    right_w + GAP * 2.0
                } else {
                    0.0
                }
        };
        let with_version = how_many_fit(&widths, room_for_items(true)) == items.len();
        let shown = how_many_fit(&widths, room_for_items(with_version));

        let mut x = bar.x + PAD;
        ui.text(label, x, mid - label_h / 2.0, TEXT_STYLE, theme.text_dim);
        x += label_w + GAP;
        for (item, w) in items.iter().zip(&widths).take(shown) {
            draw_shortcut(ui, theme, item, x, mid);
            x += w + GAP;
        }
        if with_version {
            let right = bar.x + bar.w - PAD;
            ui.text(
                &version,
                right - DOT - 8.0 - version_w,
                mid - version_h / 2.0,
                TEXT_STYLE,
                theme.text_dim,
            );
            ui.rect(
                Rect::new(right - DOT, mid - DOT / 2.0, DOT, DOT),
                DOT / 2.0,
                dot_color,
            );
        }
        STRIP_HEIGHT
    }

    fn width_of(&self, ui: &mut Ui, s: &Shortcut) -> f32 {
        ui.measure(&s.keys, KEY_STYLE).0
            + KEY_PAD * 2.0
            + KEY_TEXT_GAP
            + ui.measure(&s.label, TEXT_STYLE).0
    }
}

fn draw_shortcut(ui: &mut Ui, theme: &Theme, s: &Shortcut, x: f32, mid: f32) {
    let (key_w, key_h) = ui.measure(&s.keys, KEY_STYLE);
    let chip = Rect::new(x, mid - KEY_H / 2.0, key_w + KEY_PAD * 2.0, KEY_H);
    ui.panel(chip, KEY_H / 2.0, theme.chrome, theme.chip_line, 1.0);
    ui.text(
        &s.keys,
        chip.x + KEY_PAD,
        mid - key_h / 2.0,
        KEY_STYLE,
        theme.text,
    );
    let (_, label_h) = ui.measure(&s.label, TEXT_STYLE);
    ui.text(
        &s.label,
        chip.x + chip.w + KEY_TEXT_GAP,
        mid - label_h / 2.0,
        TEXT_STYLE,
        theme.text_dim,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_model::model;

    fn draw(m: &Model, width: u32) -> Ui {
        let mut ui = Ui::new();
        ui.begin((width, 200), 1.0);
        ShortcutStrip::new().frame(&mut ui, m);
        ui
    }

    #[test]
    fn it_lists_the_real_shortcuts_with_the_machines_own_hotkeys() {
        let mut m = model();
        m.replay.hotkey = "Ctrl+Alt+S".into();
        m.replay.record_hotkey = "Ctrl+Alt+R".into();
        let list = shortcuts(Lang::En, &m);
        let pairs: Vec<(&str, &str)> = list
            .iter()
            .map(|s| (s.keys.as_str(), s.label.as_str()))
            .collect();
        assert_eq!(
            pairs,
            [
                ("Tab", "Studio"),
                ("F", "Full screen"),
                ("Ctrl+Alt+S", "Save clip"),
                ("Ctrl+Alt+R", "Record"),
                ("Ctrl+Alt+F1-F9", "Scenes"),
            ]
        );
    }

    #[test]
    fn a_hotkey_the_machine_did_not_give_is_not_listed() {
        let mut m = model();
        m.replay.hotkey = String::new();
        m.replay.record_hotkey = String::new();
        let keys: Vec<String> = shortcuts(Lang::Es, &m)
            .into_iter()
            .map(|s| s.keys)
            .collect();
        assert_eq!(keys, ["Tab", "F", "Ctrl+Alt+F1-F9"]);
        assert_eq!(shortcuts(Lang::Es, &m)[1].label, "Pantalla completa");
    }

    #[test]
    fn the_strip_draws_and_takes_its_height() {
        let m = model();
        let mut ui = Ui::new();
        ui.begin((1280, 200), 1.0);
        assert_eq!(ShortcutStrip::new().frame(&mut ui, &m), STRIP_HEIGHT);
        assert!(!ui.vertices().is_empty());
    }

    #[test]
    fn a_narrow_window_drops_the_version_first_and_then_the_last_shortcuts() {
        let mut m = model();
        m.replay.hotkey = "Ctrl+Alt+S".into();
        m.replay.record_hotkey = "Ctrl+Alt+R".into();
        let wide = draw(&m, 1400).vertices().len();
        let medium = draw(&m, 700).vertices().len();
        let narrow = draw(&m, 330).vertices().len();
        assert!(wide > medium && medium > narrow, "{wide} {medium} {narrow}");
    }

    #[test]
    fn the_fit_rule_counts_gaps_between_items() {
        assert_eq!(how_many_fit(&[50.0, 50.0, 50.0], 150.0), 2); // 50+14+50 = 114, +14+50 = 178
        assert_eq!(how_many_fit(&[50.0, 50.0, 50.0], 178.0), 3);
        assert_eq!(how_many_fit(&[50.0], 49.0), 0);
        assert_eq!(how_many_fit(&[], 100.0), 0);
    }

    #[test]
    fn the_dot_follows_the_state_of_the_picture() {
        let mut m = model();
        m.status = StatusKind::Problem;
        let bad = draw(&m, 1280);
        m.status = StatusKind::Live;
        let good = draw(&m, 1280);
        assert_ne!(bad.vertices(), good.vertices());
    }

    #[test]
    fn it_survives_a_tiny_window() {
        let m = model();
        let ui = draw(&m, 40);
        let _ = ui.vertices();
    }
}
