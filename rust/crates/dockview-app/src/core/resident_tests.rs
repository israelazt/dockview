//! The capture hotkey and the tray icon of `Core`, on fakes.

use std::time::Duration;

use dockview_domain::events::AppEvent;
use dockview_domain::hotkey::HotkeySpec;
use dockview_domain::select::Mode;
use dockview_domain::tray::{MenuEntry, TrayEvent};

use crate::cli::Cli;
use crate::testing::rig::Rig;

fn print_screen() -> HotkeySpec {
    HotkeySpec::DEFAULT_CAPTURE
}

fn tick(rig: &mut Rig) {
    rig.advance(Duration::from_millis(50));
}

#[test]
fn at_start_the_hotkey_is_registered_and_the_tray_is_shown_in_the_language() {
    let rig = Rig::new();
    let world = rig.resident.lock().unwrap();
    assert_eq!(world.registered, [print_screen()]);
    assert_eq!(world.tray_alive, 1);
    assert_eq!(world.tray_shown.len(), 1);
    assert!(world.tray_shown[0]
        .menu
        .contains(&MenuEntry::Heading("New screenshot".into())));
    drop(world);
    assert!(rig.core.keeps_in_tray());
}

#[test]
fn the_hotkey_starts_a_capture_in_the_mode_chosen() {
    let mut rig = Rig::new();
    rig.resident.lock().unwrap().pressed = true;
    tick(&mut rig);
    assert_eq!(rig.host.lock().unwrap().captures, [Mode::Rectangle]);
    // One press is one capture.
    tick(&mut rig);
    assert_eq!(rig.host.lock().unwrap().captures.len(), 1);

    rig.core.settings.capture.mode = Mode::Freeform;
    rig.resident.lock().unwrap().pressed = true;
    tick(&mut rig);
    assert_eq!(
        rig.host.lock().unwrap().captures,
        [Mode::Rectangle, Mode::Freeform]
    );
}

#[test]
fn what_the_user_does_with_the_icon_becomes_what_the_window_does() {
    let mut rig = Rig::new();
    rig.resident.lock().unwrap().tray_events = vec![
        TrayEvent::Open,
        TrayEvent::Capture(Mode::Window),
        TrayEvent::Quit,
    ];
    tick(&mut rig);
    let host = rig.host.lock().unwrap();
    assert_eq!(host.shown, 1);
    assert_eq!(host.captures, [Mode::Window]);
    assert_eq!(host.quits, 1);
}

#[test]
fn a_new_hotkey_replaces_the_old_one_which_is_given_back_first() {
    let mut rig = Rig::new();
    let ctrl_f10 = HotkeySpec::parse("Ctrl+F10").unwrap();
    rig.core.settings.capture.hotkey = Some(ctrl_f10);
    tick(&mut rig);
    {
        let world = rig.resident.lock().unwrap();
        assert_eq!(world.registered, [print_screen(), ctrl_f10]);
        assert_eq!(world.released, [print_screen()]);
    }
    // No hotkey at all gives it back and registers nothing.
    rig.core.settings.capture.hotkey = None;
    tick(&mut rig);
    let world = rig.resident.lock().unwrap();
    assert_eq!(world.released, [print_screen(), ctrl_f10]);
    assert_eq!(world.registered.len(), 2);
}

#[test]
fn a_hotkey_another_program_has_is_said_once_and_not_retried_every_tick() {
    let mut rig = Rig::new();
    let taken = HotkeySpec::parse("Ctrl+F11").unwrap();
    rig.resident.lock().unwrap().busy = vec![taken];
    rig.core.take_events();
    rig.core.settings.capture.hotkey = Some(taken);
    tick(&mut rig);
    tick(&mut rig);
    assert_eq!(rig.core.take_events(), [AppEvent::CaptureHotkeyBusy]);
    // It does not fire, and a press is ignored.
    rig.resident.lock().unwrap().pressed = true;
    tick(&mut rig);
    assert!(rig.host.lock().unwrap().captures.is_empty());
}

#[test]
fn a_key_the_system_keeps_is_registered_and_the_user_is_told_once() {
    let mut rig = Rig::new();
    let kept = HotkeySpec::parse("Ctrl+F12").unwrap();
    rig.resident.lock().unwrap().kept = vec![kept];
    rig.core.take_events();
    rig.core.settings.capture.hotkey = Some(kept);
    tick(&mut rig);
    tick(&mut rig);
    assert_eq!(rig.core.take_events(), [AppEvent::PrintScreenIsWindows]);
    assert!(rig.resident.lock().unwrap().registered.contains(&kept));
}

#[test]
fn the_tray_comes_and_goes_with_the_setting_and_follows_the_language() {
    let mut rig = Rig::new();
    rig.core.settings.capture.keep_in_tray = false;
    tick(&mut rig);
    assert_eq!(rig.resident.lock().unwrap().tray_alive, 0);
    assert!(!rig.core.keeps_in_tray());

    rig.core.settings.capture.keep_in_tray = true;
    tick(&mut rig);
    assert_eq!(rig.resident.lock().unwrap().tray_alive, 1);

    // A new language puts a new icon with the new words, not a second icon.
    rig.core.settings.language = dockview_domain::settings::LangMode::Es;
    tick(&mut rig);
    let world = rig.resident.lock().unwrap();
    assert_eq!(world.tray_alive, 1);
    assert!(world
        .tray_shown
        .last()
        .unwrap()
        .menu
        .contains(&MenuEntry::Heading("Nueva captura".into())));
}

#[test]
fn a_tray_that_cannot_be_shown_is_not_retried_every_tick_and_closing_then_quits() {
    let rig = {
        let mut rig = Rig::new();
        rig.resident.lock().unwrap().tray_fails = true;
        rig.core.settings.capture.keep_in_tray = false;
        tick(&mut rig);
        rig.core.settings.capture.keep_in_tray = true;
        tick(&mut rig);
        tick(&mut rig);
        rig
    };
    assert!(
        !rig.core.keeps_in_tray(),
        "with no icon, closing the window must quit"
    );
    assert_eq!(
        rig.resident.lock().unwrap().tray_shown.len(),
        1,
        "only the first one, at start"
    );
}

#[test]
fn an_interface_preview_touches_no_hotkey_and_no_tray() {
    let rig = Rig::builder()
        .cli(Cli {
            ui_preview: true,
            ..Cli::default()
        })
        .build();
    let world = rig.resident.lock().unwrap();
    assert!(world.registered.is_empty());
    assert_eq!(world.tray_alive, 0);
}
