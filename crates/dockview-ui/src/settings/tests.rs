use super::*;
use crate::test_model::model;
use dockview_domain::settings::{LangMode, RenderPreset, ThemeMode, VideoModePref};

const AREA: Rect = Rect {
    x: 0.0,
    y: 0.0,
    w: 1000.0,
    h: 640.0,
};

fn frame(page: &mut SettingsPage, m: &Model, pointer: &Pointer) -> Ui {
    let mut ui = Ui::new();
    ui.begin((1000, 640), 1.0);
    page.frame(&mut ui, m, AREA, pointer, 0.016);
    ui
}

fn press_at(x: f32, y: f32) -> Pointer {
    Pointer {
        pos: Some((x, y)),
        down: true,
        just_pressed: true,
        ..Pointer::default()
    }
}

/// The page after one frame, so that it knows what can be clicked.
fn ready(m: &Model) -> SettingsPage {
    let mut page = SettingsPage::new();
    page.set_animations(false);
    frame(&mut page, m, &Pointer::default());
    page
}

fn click(page: &mut SettingsPage, m: &Model, item: usize) -> Vec<Action> {
    let (r, _) = &page.before[item];
    let (x, y) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
    frame(page, m, &press_at(x, y));
    page.take_actions()
}

fn settings(a: SettingsAction) -> Action {
    Action::Settings(a)
}

#[test]
fn the_page_offers_twelve_things_in_the_order_it_draws_them() {
    // quality 3, video mode 2, theme 3, language 3 and the tray switch.
    assert_eq!(ready(&model()).before.len(), 12);
}

#[test]
fn each_choice_asks_for_its_own_setting_and_nothing_else() {
    let m = model();
    let mut page = ready(&m);
    assert_eq!(
        click(&mut page, &m, 2),
        [settings(SettingsAction::SetPreset(RenderPreset::Quality))]
    );
    assert_eq!(
        click(&mut page, &m, 4),
        [settings(SettingsAction::SetVideoMode(
            VideoModePref::Compressed
        ))]
    );
    assert_eq!(
        click(&mut page, &m, 6),
        [settings(SettingsAction::SetTheme(ThemeMode::Dark))]
    );
    assert_eq!(
        click(&mut page, &m, 10),
        [settings(SettingsAction::SetLang(LangMode::Es))]
    );
}

#[test]
fn the_tray_switch_asks_for_the_opposite_of_what_it_shows() {
    let mut m = model();
    m.settings.keep_in_tray = true;
    let mut page = ready(&m);
    assert_eq!(
        click(&mut page, &m, 11),
        [settings(SettingsAction::SetKeepInTray(false))]
    );
    m.settings.keep_in_tray = false;
    let mut page = ready(&m);
    assert_eq!(
        click(&mut page, &m, 11),
        [settings(SettingsAction::SetKeepInTray(true))]
    );
}

#[test]
fn a_press_on_nothing_asks_for_nothing_and_the_pointer_is_not_wanted() {
    let m = model();
    let mut page = ready(&m);
    frame(&mut page, &m, &press_at(2.0, 2.0));
    assert!(page.take_actions().is_empty());
    assert!(!page.wants_pointer());
}

#[test]
fn the_hand_cursor_is_wanted_over_a_choice_only() {
    let m = model();
    let mut page = ready(&m);
    let (r, _) = page.before[0].clone();
    let over = Pointer {
        pos: Some((r.x + 3.0, r.y + 3.0)),
        ..Pointer::default()
    };
    frame(&mut page, &m, &over);
    assert!(page.wants_pointer());
}

#[test]
fn the_page_draws_the_same_things_in_both_languages_and_themes() {
    let mut m = model();
    let en = frame(&mut SettingsPage::new(), &m, &Pointer::default());
    m.lang = dockview_domain::i18n::Lang::Es;
    m.theme = dockview_domain::theme::Theme::light();
    let es = frame(&mut SettingsPage::new(), &m, &Pointer::default());
    assert!(!en.vertices().is_empty() && !es.vertices().is_empty());
    assert_ne!(en.vertices(), es.vertices());
}

#[test]
fn a_narrow_window_still_fits_the_page_inside_it() {
    let m = model();
    let mut page = SettingsPage::new();
    let mut ui = Ui::new();
    ui.begin((420, 640), 1.0);
    let area = Rect::new(0.0, 0.0, 420.0, 640.0);
    page.frame(&mut ui, &m, area, &Pointer::default(), 0.016);
    for (r, _) in &page.before {
        assert!(r.x >= 0.0 && r.x + r.w <= 420.0 + 0.5, "{r:?}");
    }
}
