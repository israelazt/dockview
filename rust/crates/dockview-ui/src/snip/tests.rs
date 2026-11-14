use super::*;
use dockview_domain::desktop::DesktopMonitor;
use dockview_domain::overlay_input::OverlayController;
use dockview_domain::select::Input;

struct Rig {
    ui: Ui,
    chrome: SnipChrome,
    controller: OverlayController,
}

impl Rig {
    fn new() -> Self {
        let monitor = DesktopMonitor {
            bounds: PxRect::new(0, 0, 1920, 1080),
            name: "M".into(),
            primary: true,
            scale_percent: 100,
        };
        let windows = vec![
            PxRect::new(100, 100, 400, 300),
            PxRect::new(700, 200, 300, 300),
        ];
        Self {
            ui: Ui::new(),
            chrome: SnipChrome::new(),
            controller: OverlayController::new(
                Mode::Rectangle,
                PxRect::new(0, 0, 1920, 1080),
                windows,
                &monitor,
            ),
        }
    }

    fn frame(&mut self, dt: f32, animations: bool) {
        self.ui.begin((1920, 1080), 1.0);
        let view = self.controller.view();
        let v = SnipView {
            view: &view,
            mode: self.controller.mode(),
            origin: (0, 0),
            toolbar: self.controller.toolbar(),
            hovered: self.controller.hovered_button(),
            pressed: self.controller.button_pressed(),
            close_hovered: self.controller.close_hovered(),
            close_pressed: self.controller.close_pressed(),
            lang: Lang::Es,
        };
        self.chrome.frame(&mut self.ui, &v, dt, animations);
    }

    fn settle(&mut self) {
        for _ in 0..60 {
            self.frame(0.016, true);
        }
    }

    fn button(&self, mode: Mode) -> (i32, i32) {
        let r = self
            .controller
            .toolbar()
            .buttons
            .iter()
            .find(|(m, _)| *m == mode)
            .unwrap()
            .1;
        (r.x + r.w as i32 / 2, r.y + r.h as i32 / 2)
    }
}

#[test]
fn the_overlay_comes_in_over_the_animation_time_and_then_is_still() {
    let mut rig = Rig::new();
    rig.frame(0.016, true);
    assert!(rig.chrome.appear > 0.0 && rig.chrome.appear < 0.2);
    assert!(rig.chrome.is_animating());
    rig.settle();
    assert_eq!(rig.chrome.appear, 1.0);
    assert!(
        !rig.chrome.is_animating(),
        "a settled overlay needs no more frames"
    );
}

#[test]
fn without_animations_everything_is_there_at_once() {
    let mut rig = Rig::new();
    rig.frame(0.016, false);
    assert_eq!(rig.chrome.appear, 1.0);
    assert!(!rig.chrome.is_animating());
    assert!(!rig.ui.vertices().is_empty());
}

#[test]
fn the_screen_is_dimmed_everywhere_until_something_is_chosen() {
    let mut rig = Rig::new();
    rig.frame(0.016, false);
    let dimmed_only = rig.ui.vertices().len();
    // Dragging a rectangle clears it (four strips around it) and adds its
    // border and its size chip.
    rig.controller.apply(Input::Down(300, 300));
    rig.controller.apply(Input::Move(600, 500));
    rig.frame(0.016, false);
    assert!(rig.ui.vertices().len() > dimmed_only + 6 * 4);
}

#[test]
fn the_indicator_glides_to_the_new_mode_and_the_button_under_the_pointer_lights() {
    let mut rig = Rig::new();
    rig.settle();
    assert_eq!(rig.chrome.indicator, Some(0.0));
    let (x, y) = rig.button(Mode::Freeform);
    rig.controller.apply(Input::Down(x, y));
    rig.controller.apply(Input::Up(x, y));
    rig.frame(0.016, true);
    let half_way = rig.chrome.indicator.unwrap();
    assert!(half_way > 0.0 && half_way < 3.0, "{half_way}");
    rig.settle();
    assert!((rig.chrome.indicator.unwrap() - 3.0).abs() < 0.01);
    // The pointer on another button lights it, and leaving dims it again.
    let (wx, wy) = rig.button(Mode::Window);
    rig.controller.apply(Input::Move(wx, wy));
    rig.settle();
    assert!(rig.chrome.hover[1] > 0.99);
    rig.controller.apply(Input::Move(5, 900));
    rig.settle();
    assert!(rig.chrome.hover[1] < 0.01);
}

#[test]
fn the_window_highlight_glides_from_one_window_to_the_next() {
    let mut rig = Rig::new();
    rig.controller.apply(Input::SetMode(Mode::Window));
    rig.controller.apply(Input::Move(150, 150));
    rig.settle();
    let first = rig.chrome.window.unwrap();
    assert!((first[0] - 100.0).abs() < 1.0 && (first[2] - 400.0).abs() < 1.0);
    rig.controller.apply(Input::Move(800, 300));
    rig.frame(0.016, true);
    let between = rig.chrome.window.unwrap();
    assert!(between[0] > 100.0 && between[0] < 700.0, "{between:?}");
    rig.settle();
    assert!((rig.chrome.window.unwrap()[0] - 700.0).abs() < 1.0);
    // With no window under the pointer the highlight fades out.
    rig.controller.apply(Input::Move(1800, 1000));
    rig.settle();
    assert!(rig.chrome.window_alpha < 0.01);
}

#[test]
fn the_buttons_are_named_in_the_language_asked_and_have_icons() {
    use dockview_domain::i18n::Key;
    assert_eq!(tr(Lang::Es, Key::SnapModeFree), "Forma libre");
    for mode in Mode::ALL {
        assert!(
            icon_of(mode) as u32 >= 0xE000,
            "{mode:?} has a private-use icon"
        );
    }
    let icons: std::collections::HashSet<char> = Mode::ALL.iter().map(|m| icon_of(*m)).collect();
    assert_eq!(icons.len(), 4, "each mode has its own icon");
}

#[test]
fn a_free_form_path_is_drawn_as_dots() {
    let mut rig = Rig::new();
    rig.controller.apply(Input::SetMode(Mode::Freeform));
    rig.frame(0.016, false);
    let before = rig.ui.vertices().len();
    rig.controller.apply(Input::Down(100, 100));
    for p in [(120, 110), (150, 140), (180, 200)] {
        rig.controller.apply(Input::Move(p.0, p.1));
    }
    rig.frame(0.016, false);
    assert!(rig.ui.vertices().len() > before + 3 * 4);
}

#[test]
fn drawing_scales_with_the_monitor_scale() {
    let mut rig = Rig::new();
    // At 200 % the logical viewport is half the physical one; nothing breaks
    // and the bar still draws.
    rig.ui.begin((1920, 1080), 2.0);
    let view = rig.controller.view();
    let v = SnipView {
        view: &view,
        mode: Mode::Rectangle,
        origin: (0, 0),
        toolbar: rig.controller.toolbar(),
        hovered: None,
        pressed: None,
        close_hovered: false,
        close_pressed: false,
        lang: Lang::En,
    };
    rig.chrome.frame(&mut rig.ui, &v, 0.016, false);
    assert!(!rig.ui.vertices().is_empty());
}

#[test]
fn the_capsule_is_as_round_as_the_pill_of_the_app_and_has_a_close_button_that_lights() {
    let mut rig = Rig::new();
    rig.settle();
    // Half its height: a full capsule, like the pill (radius 28 on 56).
    let back = rig.controller.toolbar().back;
    assert_eq!(back.h, 56);
    let close = rig.controller.toolbar().close;
    rig.controller.apply(Input::Move(close.x + 5, close.y + 5));
    rig.settle();
    assert!(
        rig.chrome.hover[4] > 0.99,
        "the close button lights under the pointer"
    );
    assert!(rig.chrome.hover[0] < 0.01);
    rig.controller.apply(Input::Move(5, 900));
    rig.settle();
    assert!(rig.chrome.hover[4] < 0.01);
}
