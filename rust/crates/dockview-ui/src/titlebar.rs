//! The title bar of the window, which is ours: the logo, the name, the version
//! and the three window buttons. It only draws; what a click does is decided by
//! the zones of `dockview_domain::window_chrome` and the shell.

use dockview_domain::brand::title_text;
use dockview_domain::i18n::{tr, Key, Lang};
use dockview_domain::theme::{Color, Theme};
use dockview_domain::window_chrome::{WindowButton, BUTTON_WIDTH, TITLE_BAR_HEIGHT};

use super::text::Weight;
use super::{Rect, TextStyle, Ui};

const ICON_MINIMIZE: char = '\u{e11c}';
const ICON_MAXIMIZE: char = '\u{e167}';
const ICON_RESTORE: char = '\u{e09e}';
const ICON_CLOSE: char = '\u{e1b2}';

const LEFT_PAD: f32 = 12.0;
const LOGO_SIZE: f32 = 18.0;
const TITLE_STYLE: TextStyle = TextStyle::new(12.0, Weight::SemiBold);
/// Extra space between the letters of the name, in logical pixels.
const TRACKING: f32 = 1.8;
const CHIP_STYLE: TextStyle = TextStyle::new(11.0, Weight::Regular);
const CHIP_HEIGHT: f32 = 20.0;
const CHIP_PAD: f32 = 10.0;
const ICON_SIZE: f32 = 14.0;
/// The red every Windows close button turns on hover.
const CLOSE_HOVER: Color = Color::hex(0xE81123, 1.0);
const CLOSE_PRESSED: Color = Color::hex(0xB50E1C, 1.0);

/// What the pointer is doing over the buttons, and the state of the window.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TitleBarState {
    pub hover: Option<WindowButton>,
    pub pressed: Option<WindowButton>,
    pub maximized: bool,
}

/// Draws the bar across the top of the window and returns its height in
/// logical pixels. `version` is the label next to the name.
pub fn draw(ui: &mut Ui, theme: &Theme, lang: Lang, state: &TitleBarState, version: &str) -> f32 {
    let (width, _) = ui.logical_size();
    let bar = Rect::new(0.0, 0.0, width, TITLE_BAR_HEIGHT);
    ui.rect(bar, 0.0, theme.chrome);
    ui.rect(
        Rect::new(0.0, TITLE_BAR_HEIGHT - 1.0, width, 1.0),
        0.0,
        theme.chrome_line,
    );

    let mid = (TITLE_BAR_HEIGHT - 1.0) / 2.0;
    let mut x = LEFT_PAD;
    draw_logo(ui, theme, x, mid);
    x += LOGO_SIZE + 10.0;
    x += draw_tracked(ui, &title_text(), x, mid, theme.text) + 12.0;
    draw_chip(ui, theme, version, x, mid);

    for button in WindowButton::ALL {
        draw_button(ui, theme, lang, state, button, width);
    }
    TITLE_BAR_HEIGHT
}

/// A provisional mark: a rounded square with a light in it.
fn draw_logo(ui: &mut Ui, theme: &Theme, x: f32, mid: f32) {
    let r = Rect::new(x, mid - LOGO_SIZE / 2.0, LOGO_SIZE, LOGO_SIZE);
    ui.rect(r, 5.0, theme.brand);
    let core = LOGO_SIZE * 0.42;
    let (cx, cy) = r.center();
    ui.rect(
        Rect::new(cx - core / 2.0, cy - core / 2.0, core, core),
        core / 2.0,
        theme.chrome,
    );
}

/// Text with a little extra space between the letters; returns its width.
fn draw_tracked(ui: &mut Ui, text: &str, x: f32, mid: f32, color: Color) -> f32 {
    let (_, height) = ui.measure(text, TITLE_STYLE);
    let y = mid - height / 2.0;
    let mut cursor = x;
    for letter in text.chars() {
        let s = letter.to_string();
        cursor += ui.text(&s, cursor, y, TITLE_STYLE, color) + TRACKING;
    }
    cursor - x - TRACKING
}

fn draw_chip(ui: &mut Ui, theme: &Theme, text: &str, x: f32, mid: f32) {
    let (w, h) = ui.measure(text, CHIP_STYLE);
    let r = Rect::new(x, mid - CHIP_HEIGHT / 2.0, w + CHIP_PAD * 2.0, CHIP_HEIGHT);
    ui.panel(r, 8.0, theme.chip, theme.chip_line, 1.0);
    ui.text(
        text,
        r.x + CHIP_PAD,
        mid - h / 2.0,
        CHIP_STYLE,
        theme.text_dim,
    );
}

fn draw_button(
    ui: &mut Ui,
    theme: &Theme,
    _lang: Lang,
    state: &TitleBarState,
    button: WindowButton,
    width: f32,
) {
    let slot = match button {
        WindowButton::Minimize => 2.0,
        WindowButton::Maximize => 1.0,
        WindowButton::Close => 0.0,
    };
    let r = Rect::new(
        width - BUTTON_WIDTH * (slot + 1.0),
        0.0,
        BUTTON_WIDTH,
        TITLE_BAR_HEIGHT - 1.0,
    );
    let hovered = state.hover == Some(button);
    let pressed = state.pressed == Some(button) && hovered;
    let (fill, icon) = match (button, hovered, pressed) {
        (WindowButton::Close, _, true) => (Some(CLOSE_PRESSED), Color::hex(0xFFFFFF, 1.0)),
        (WindowButton::Close, true, _) => (Some(CLOSE_HOVER), Color::hex(0xFFFFFF, 1.0)),
        (_, _, true) => (Some(theme.segment_active), theme.text),
        (_, true, _) => (Some(theme.hover), theme.text),
        _ => (None, theme.text_dim),
    };
    if let Some(fill) = fill {
        ui.rect(r, 0.0, fill);
    }
    let glyph = match button {
        WindowButton::Minimize => ICON_MINIMIZE,
        WindowButton::Maximize if state.maximized => ICON_RESTORE,
        WindowButton::Maximize => ICON_MAXIMIZE,
        WindowButton::Close => ICON_CLOSE,
    };
    let (cx, cy) = r.center();
    ui.icon(glyph, cx, cy, ICON_SIZE, icon);
}

/// The words for the window button's tooltip.
pub fn button_label(lang: Lang, button: WindowButton, maximized: bool) -> &'static str {
    tr(
        lang,
        match button {
            WindowButton::Minimize => Key::WindowMinimize,
            WindowButton::Maximize if maximized => Key::WindowRestore,
            WindowButton::Maximize => Key::WindowMaximize,
            WindowButton::Close => Key::WindowClose,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use dockview_domain::window_chrome::{Chrome, Zone};

    fn frame(theme: &Theme, state: TitleBarState, size: (u32, u32), scale: f32) -> Ui {
        let mut ui = Ui::new();
        ui.begin(size, scale);
        draw(&mut ui, theme, Lang::En, &state, "v0.1.0 · D3D11");
        ui
    }

    fn has_color(ui: &Ui, c: Color) -> bool {
        ui.vertices().iter().any(|v| {
            (v.color[0] - c.r).abs() < 0.01
                && (v.color[1] - c.g).abs() < 0.01
                && (v.color[2] - c.b).abs() < 0.01
        })
    }

    #[test]
    fn the_bar_draws_and_returns_its_height() {
        let mut ui = Ui::new();
        ui.begin((1280, 720), 1.0);
        let h = draw(
            &mut ui,
            &Theme::dark(),
            Lang::En,
            &TitleBarState::default(),
            "v1",
        );
        assert_eq!(h, TITLE_BAR_HEIGHT);
        assert!(!ui.vertices().is_empty());
    }

    #[test]
    fn nothing_is_drawn_below_the_bar() {
        let ui = frame(&Theme::dark(), TitleBarState::default(), (1280, 720), 1.0);
        let lowest = ui
            .vertices()
            .iter()
            .map(|v| v.pos[1])
            .fold(0.0_f32, f32::max);
        // Rounded shapes are drawn a couple of pixels wider for their soft edge.
        assert!(lowest <= TITLE_BAR_HEIGHT + 3.0, "{lowest}");
    }

    #[test]
    fn the_bar_follows_the_scale() {
        let ui = frame(&Theme::dark(), TitleBarState::default(), (2560, 1440), 2.0);
        let lowest = ui
            .vertices()
            .iter()
            .map(|v| v.pos[1])
            .fold(0.0_f32, f32::max);
        assert!((lowest - TITLE_BAR_HEIGHT * 2.0).abs() < 6.0, "{lowest}");
    }

    #[test]
    fn closing_turns_red_only_on_hover_and_darker_when_pressed() {
        let theme = Theme::dark();
        let plain = frame(&theme, TitleBarState::default(), (1280, 720), 1.0);
        assert!(!has_color(&plain, CLOSE_HOVER));
        let hover = TitleBarState {
            hover: Some(WindowButton::Close),
            ..TitleBarState::default()
        };
        assert!(has_color(
            &frame(&theme, hover, (1280, 720), 1.0),
            CLOSE_HOVER
        ));
        let pressed = TitleBarState {
            pressed: Some(WindowButton::Close),
            ..hover
        };
        assert!(has_color(
            &frame(&theme, pressed, (1280, 720), 1.0),
            CLOSE_PRESSED
        ));
        // Pressed, but the pointer left the button: back to normal.
        let left = TitleBarState {
            pressed: Some(WindowButton::Close),
            hover: None,
            maximized: false,
        };
        assert!(!has_color(
            &frame(&theme, left, (1280, 720), 1.0),
            CLOSE_PRESSED
        ));
    }

    #[test]
    fn maximizing_changes_the_icon_of_that_button() {
        let theme = Theme::dark();
        let normal = frame(&theme, TitleBarState::default(), (1280, 720), 1.0);
        let maximized = TitleBarState {
            maximized: true,
            ..TitleBarState::default()
        };
        let restore = frame(&theme, maximized, (1280, 720), 1.0);
        assert_ne!(normal.vertices(), restore.vertices());
    }

    #[test]
    fn both_themes_draw_and_differ() {
        let dark = frame(&Theme::dark(), TitleBarState::default(), (1280, 720), 1.0);
        let light = frame(&Theme::light(), TitleBarState::default(), (1280, 720), 1.0);
        assert!(has_color(&dark, Theme::dark().chrome));
        assert!(has_color(&light, Theme::light().chrome));
        assert!(!has_color(&dark, Theme::light().chrome));
    }

    #[test]
    fn the_buttons_are_where_the_zones_say() {
        // Drawing and hit-testing use the same numbers.
        let chrome = Chrome {
            size: (1280, 720),
            scale: 1.0,
            maximized: false,
            fullscreen: false,
            panel_open: false,
        };
        for button in WindowButton::ALL {
            let r = chrome.button(button).unwrap();
            assert_eq!(
                chrome.zone_at(r.x + r.w / 2.0, r.h / 2.0),
                Zone::Button(button)
            );
        }
    }

    #[test]
    fn the_labels_follow_the_state_and_the_language() {
        assert_eq!(
            button_label(Lang::En, WindowButton::Maximize, false),
            "Maximize"
        );
        assert_eq!(
            button_label(Lang::En, WindowButton::Maximize, true),
            "Restore"
        );
        assert_eq!(button_label(Lang::Es, WindowButton::Close, false), "Cerrar");
    }
}
