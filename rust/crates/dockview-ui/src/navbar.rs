//! The navigation row under the title bar: the four tabs on the left and the
//! status (encoder, lost pictures, stream) on the right. It draws in the row's
//! own coordinates (the shell leaves room above it) and turns clicks into
//! actions.

use dockview_domain::broadcast::StreamState;
use dockview_domain::i18n::{tr, Key, Lang};
use dockview_domain::navigation::{EncoderState, NavTab, NavView};
use dockview_domain::theme::{Color, Theme};
use dockview_domain::view::{Action, Model};
use dockview_domain::window_chrome::NAV_HEIGHT;

use super::input::Pointer;
use super::studio::StudioSection;
use super::text::Weight;
use super::{Rect, TextStyle, Ui};

const MARGIN: f32 = 18.0;
const TABS_H: f32 = 42.0;
const TAB_H: f32 = 34.0;
const TAB_PAD: f32 = 20.0;
const TAB_GAP: f32 = 4.0;
const TABS_PAD: f32 = 4.0;
const TAB_STYLE: TextStyle = TextStyle::new(14.0, Weight::Medium);
const STATUS_H: f32 = 33.0;
const STATUS_PAD: f32 = 16.0;
const ITEM_GAP: f32 = 12.0;
const STATUS_STYLE: TextStyle = TextStyle::new(12.0, Weight::Medium);
const DOT: f32 = 6.0;
/// Room kept between the tabs and the status.
const MIN_GAP: f32 = 24.0;

/// One thing in the status, before it is placed.
#[derive(Debug, Clone, PartialEq)]
struct Item {
    text: String,
    color: Color,
    dot: Option<Color>,
}

/// How many of the items (from the left) fit in `room` pixels when each is
/// `widths[i]` wide with `ITEM_GAP` between them and `STATUS_PAD` on both sides.
fn items_that_fit(widths: &[f32], room: f32) -> usize {
    let mut used = STATUS_PAD * 2.0;
    let mut count = 0;
    for (i, w) in widths.iter().enumerate() {
        let next = used + w + if i > 0 { ITEM_GAP * 2.0 + 1.0 } else { 0.0 };
        if next > room {
            break;
        }
        used = next;
        count += 1;
    }
    count
}

fn items(theme: &Theme, lang: Lang, nav: &NavView) -> Vec<Item> {
    let mut out = Vec::new();
    let word = |key| tr(lang, key);
    match nav.encoder {
        EncoderState::Unknown => {}
        EncoderState::Unavailable => out.push(Item {
            text: word(Key::NavEncoderNone).to_string(),
            color: theme.error,
            dot: Some(theme.error),
        }),
        state => {
            let state_word = word(if state == EncoderState::Active {
                Key::NavEncoderActive
            } else {
                Key::NavEncoderReady
            });
            let text = if nav.encoder_name.is_empty() {
                state_word.to_string()
            } else {
                format!("{} {}", nav.encoder_name, state_word)
            };
            out.push(Item {
                text,
                color: theme.brand,
                dot: Some(theme.brand),
            });
        }
    }
    out.push(Item {
        text: format!("{} {}", nav.drops, word(Key::NavDrops)),
        color: if nav.drops > 0 {
            theme.warn
        } else {
            theme.text
        },
        dot: None,
    });
    let (key, color) = match nav.stream {
        StreamState::Offline | StreamState::Failed => (Key::NavOffline, theme.text_dim),
        StreamState::Connecting => (Key::NavConnecting, theme.warn),
        StreamState::Reconnecting => (Key::NavReconnecting, theme.warn),
        StreamState::Live => (Key::NavLive, theme.error),
    };
    out.push(Item {
        text: word(key).to_string(),
        color,
        dot: Some(color),
    });
    out
}

#[derive(Debug, Default)]
pub struct NavBar {
    actions: Vec<Action>,
    section: Option<StudioSection>,
    /// The tab the left button went down on.
    pressed: Option<NavTab>,
    hover: Option<NavTab>,
}

impl NavBar {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn take_actions(&mut self) -> Vec<Action> {
        std::mem::take(&mut self.actions)
    }

    /// The section of the Studio panel a tab asked to be opened (see
    /// `StudioPanel::open_section`), once.
    pub fn take_studio_request(&mut self) -> Option<StudioSection> {
        self.section.take()
    }

    /// Whether the pointer is over something that can be clicked.
    pub fn wants_pointer(&self) -> bool {
        self.hover.is_some()
    }

    /// Draws the row at the top of the area the shell gave it (its height is
    /// [`NAV_HEIGHT`]) and handles the pointer, which is in the same
    /// coordinates. Returns the height it took.
    pub fn frame(&mut self, ui: &mut Ui, model: &Model, pointer: &Pointer) -> f32 {
        let (width, _) = ui.logical_size();
        let theme = &model.theme;
        let rects = self.tab_rects(ui, model.lang, width);
        self.hover = pointer.pos.and_then(|(x, y)| {
            rects
                .iter()
                .find(|(_, r)| r.contains(x, y))
                .map(|(t, _)| *t)
        });
        self.handle_clicks(pointer);

        // The tabs, in their pill.
        let tabs_top = (NAV_HEIGHT - TABS_H) / 2.0;
        let last = rects.last().map_or(MARGIN, |(_, r)| r.x + r.w);
        let tabs = Rect::new(MARGIN, tabs_top, last + TABS_PAD - MARGIN, TABS_H);
        ui.panel(tabs, TABS_H / 2.0, theme.chip, theme.chip_line, 1.0);
        for (tab, rect) in &rects {
            self.draw_tab(ui, theme, model.lang, *tab, *rect, model.nav.tab);
        }

        // The status, on the right, as much of it as fits.
        let all = items(theme, model.lang, &model.nav);
        let widths: Vec<f32> = all
            .iter()
            .map(|i| {
                ui.measure(&i.text, STATUS_STYLE).0 + if i.dot.is_some() { DOT + 8.0 } else { 0.0 }
            })
            .collect();
        let room = width - tabs.x - tabs.w - MIN_GAP - MARGIN;
        let shown = items_that_fit(&widths, room);
        if shown > 0 {
            draw_status(ui, theme, &all[..shown], &widths[..shown], width);
        }
        NAV_HEIGHT
    }

    fn handle_clicks(&mut self, pointer: &Pointer) {
        if pointer.just_pressed {
            self.pressed = self.hover;
        }
        if pointer.just_released {
            if let (Some(pressed), Some(over)) = (self.pressed.take(), self.hover) {
                if pressed == over {
                    self.actions.push(Action::GoTo(over));
                    self.section = match over {
                        NavTab::Recordings => Some(StudioSection::Recording),
                        NavTab::Settings => Some(StudioSection::Settings),
                        _ => None,
                    };
                }
            }
            self.pressed = None;
        }
    }

    fn tab_rects(&self, ui: &mut Ui, lang: Lang, _width: f32) -> Vec<(NavTab, Rect)> {
        let mut x = MARGIN + TABS_PAD;
        let top = (NAV_HEIGHT - TAB_H) / 2.0;
        NavTab::ALL
            .into_iter()
            .map(|tab| {
                let w = ui.measure(tr(lang, tab.title()), TAB_STYLE).0 + TAB_PAD * 2.0;
                let rect = Rect::new(x, top, w, TAB_H);
                x += w + TAB_GAP;
                (tab, rect)
            })
            .collect()
    }

    fn draw_tab(&self, ui: &mut Ui, theme: &Theme, lang: Lang, tab: NavTab, r: Rect, on: NavTab) {
        let active = tab == on;
        let hovered = self.hover == Some(tab);
        if active {
            ui.rect(r, r.h / 2.0, theme.brand);
        } else if hovered {
            ui.rect(r, r.h / 2.0, theme.hover);
        }
        let color = if active {
            theme.on_brand
        } else if hovered {
            theme.text
        } else {
            theme.text_dim
        };
        let text = tr(lang, tab.title());
        let (w, h) = ui.measure(text, TAB_STYLE);
        ui.text(
            text,
            r.x + (r.w - w) / 2.0,
            r.y + (r.h - h) / 2.0,
            TAB_STYLE,
            color,
        );
    }
}

fn draw_status(ui: &mut Ui, theme: &Theme, items: &[Item], widths: &[f32], width: f32) {
    let total: f32 = STATUS_PAD * 2.0
        + widths.iter().sum::<f32>()
        + (items.len().saturating_sub(1)) as f32 * (ITEM_GAP * 2.0 + 1.0);
    let top = (NAV_HEIGHT - STATUS_H) / 2.0;
    let r = Rect::new(width - MARGIN - total, top, total, STATUS_H);
    ui.panel(r, STATUS_H / 2.0, theme.chip, theme.chip_line, 1.0);
    let mid = r.y + r.h / 2.0;
    let mut x = r.x + STATUS_PAD;
    for (i, (item, w)) in items.iter().zip(widths).enumerate() {
        if i > 0 {
            x += ITEM_GAP;
            ui.rect(Rect::new(x, mid - 7.0, 1.0, 14.0), 0.0, theme.chip_line);
            x += 1.0 + ITEM_GAP;
        }
        let mut text_x = x;
        if let Some(dot) = item.dot {
            ui.rect(Rect::new(x, mid - DOT / 2.0, DOT, DOT), DOT / 2.0, dot);
            text_x += DOT + 8.0;
        }
        let (_, h) = ui.measure(&item.text, STATUS_STYLE);
        ui.text(&item.text, text_x, mid - h / 2.0, STATUS_STYLE, item.color);
        x += w;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_model::model;

    fn pointer_at(x: f32, y: f32) -> Pointer {
        Pointer {
            pos: Some((x, y)),
            ..Pointer::default()
        }
    }

    fn draw(bar: &mut NavBar, m: &Model, width: u32, pointer: &Pointer) -> Ui {
        let mut ui = Ui::new();
        ui.begin((width, 400), 1.0);
        bar.frame(&mut ui, m, pointer);
        ui
    }

    /// The centre of the tab, as the row lays it out.
    fn centre_of(m: &Model, tab: NavTab) -> (f32, f32) {
        let mut ui = Ui::new();
        ui.begin((1280, 400), 1.0);
        let rects = NavBar::new().tab_rects(&mut ui, m.lang, 1280.0);
        rects.iter().find(|(t, _)| *t == tab).unwrap().1.center()
    }

    fn click(bar: &mut NavBar, m: &Model, at: (f32, f32)) {
        let mut p = pointer_at(at.0, at.1);
        draw(bar, m, 1280, &p);
        p.press();
        draw(bar, m, 1280, &p);
        p.end_frame();
        p.release();
        draw(bar, m, 1280, &p);
    }

    #[test]
    fn the_row_draws_and_takes_its_height() {
        let m = model();
        let mut ui = Ui::new();
        ui.begin((1280, 400), 1.0);
        assert_eq!(
            NavBar::new().frame(&mut ui, &m, &Pointer::default()),
            NAV_HEIGHT
        );
        assert!(!ui.vertices().is_empty());
    }

    #[test]
    fn clicking_a_tab_goes_to_it() {
        let mut m = model();
        m.lang = Lang::En;
        let mut bar = NavBar::new();
        click(&mut bar, &m, centre_of(&m, NavTab::Studio));
        assert_eq!(bar.take_actions(), [Action::GoTo(NavTab::Studio)]);
        assert_eq!(bar.take_studio_request(), None);
        click(&mut bar, &m, centre_of(&m, NavTab::Viewer));
        assert_eq!(bar.take_actions(), [Action::GoTo(NavTab::Viewer)]);
    }

    #[test]
    fn recordings_and_settings_ask_for_their_section_of_the_panel() {
        let m = model();
        let mut bar = NavBar::new();
        click(&mut bar, &m, centre_of(&m, NavTab::Recordings));
        assert_eq!(bar.take_actions(), [Action::GoTo(NavTab::Recordings)]);
        assert_eq!(bar.take_studio_request(), Some(StudioSection::Recording));
        assert_eq!(bar.take_studio_request(), None, "asked once");
        click(&mut bar, &m, centre_of(&m, NavTab::Settings));
        assert_eq!(bar.take_studio_request(), Some(StudioSection::Settings));
    }

    #[test]
    fn a_click_that_ends_on_another_tab_or_nowhere_does_nothing() {
        let m = model();
        let mut bar = NavBar::new();
        let a = centre_of(&m, NavTab::Studio);
        let b = centre_of(&m, NavTab::Settings);
        let mut p = pointer_at(a.0, a.1);
        draw(&mut bar, &m, 1280, &p);
        p.press();
        draw(&mut bar, &m, 1280, &p);
        p.end_frame();
        p.pos = Some(b);
        draw(&mut bar, &m, 1280, &p);
        p.release();
        draw(&mut bar, &m, 1280, &p);
        assert!(bar.take_actions().is_empty());
        click(&mut bar, &m, (640.0, 300.0));
        assert!(bar.take_actions().is_empty());
    }

    #[test]
    fn hovering_a_tab_asks_for_the_hand_cursor() {
        let m = model();
        let mut bar = NavBar::new();
        let a = centre_of(&m, NavTab::Studio);
        draw(&mut bar, &m, 1280, &pointer_at(a.0, a.1));
        assert!(bar.wants_pointer());
        draw(&mut bar, &m, 1280, &pointer_at(900.0, 5.0));
        assert!(!bar.wants_pointer());
    }

    #[test]
    fn the_tab_that_is_on_is_filled_with_the_brand_colour() {
        let mut m = model();
        m.nav.tab = NavTab::Studio;
        let ui = draw(&mut NavBar::new(), &m, 1280, &Pointer::default());
        let c = m.theme.brand;
        let filled = |ui: &Ui| {
            ui.vertices().iter().any(|v| {
                (v.color[0] - c.r).abs() < 0.01
                    && (v.color[1] - c.g).abs() < 0.01
                    && (v.color[2] - c.b).abs() < 0.01
            })
        };
        assert!(filled(&ui));
        // Both of the tabs (and the status) differ when the tab changes.
        m.nav.tab = NavTab::Settings;
        let other = draw(&mut NavBar::new(), &m, 1280, &Pointer::default());
        assert_ne!(ui.vertices(), other.vertices());
    }

    #[test]
    fn the_status_says_what_the_machine_reports() {
        let theme = Theme::dark();
        let nav = NavView {
            encoder: EncoderState::Active,
            encoder_name: "NVENC".into(),
            drops: 3,
            stream: StreamState::Live,
            ..NavView::default()
        };
        let list = items(&theme, Lang::En, &nav);
        let texts: Vec<&str> = list.iter().map(|i| i.text.as_str()).collect();
        assert_eq!(texts, ["NVENC ACTIVE", "3 DROPS", "LIVE"]);
        assert_eq!(list[1].color, theme.warn, "lost pictures stand out");
        assert_eq!(list[2].color, theme.error, "live is red");
        let quiet = items(&theme, Lang::Es, &NavView::default());
        let texts: Vec<&str> = quiet.iter().map(|i| i.text.as_str()).collect();
        assert_eq!(
            texts,
            ["0 PÉRDIDAS", "Sin emitir"],
            "an unknown encoder is not shown"
        );
    }

    #[test]
    fn a_missing_encoder_is_shown_as_an_error() {
        let nav = NavView {
            encoder: EncoderState::Unavailable,
            ..NavView::default()
        };
        let list = items(&Theme::dark(), Lang::En, &nav);
        assert_eq!(list[0].text, "NO ENCODER");
        assert_eq!(list[0].color, Theme::dark().error);
    }

    #[test]
    fn a_narrow_window_loses_the_status_from_the_right_end() {
        let widths = [90.0, 60.0, 50.0];
        // 32 padding + 90 + (24+1+60) + (24+1+50) = 281
        assert_eq!(items_that_fit(&widths, 300.0), 3);
        assert_eq!(items_that_fit(&widths, 210.0), 2);
        assert_eq!(items_that_fit(&widths, 200.0), 1);
        assert_eq!(items_that_fit(&widths, 130.0), 1);
        assert_eq!(items_that_fit(&widths, 100.0), 0);
        assert_eq!(items_that_fit(&[], 500.0), 0);
    }

    #[test]
    fn a_very_narrow_window_still_draws_the_tabs_without_panicking() {
        let m = model();
        let mut bar = NavBar::new();
        let ui = draw(&mut bar, &m, 300, &Pointer::default());
        assert!(!ui.vertices().is_empty());
    }
}
