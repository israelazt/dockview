//! Drawing the window's own frame: the tabs and the status, the icon rail, the
//! shortcuts strip and the title bar, over everything else.

use dockview_ui::input::Pointer;
use dockview_ui::navbar::NavBar;
use dockview_ui::rail::Rail;
use dockview_ui::strip::ShortcutStrip;
use dockview_ui::studio::StudioSection;
use dockview_ui::titlebar::TitleBarState;
use dockview_ui::Ui;

use dockview_domain::view::Model;
use dockview_domain::window_chrome::{Chrome, RAIL_WIDTH};

/// The pieces of the frame and what they need to know about the pointer.
pub struct Parts<'a> {
    pub navbar: &'a mut NavBar,
    pub rail: &'a mut Rail,
    pub strip: &'a mut ShortcutStrip,
    pub nav_pointer: &'a Pointer,
    pub rail_pointer: &'a Pointer,
    /// The section of the Studio panel the rail marks as the one on show.
    pub rail_active: Option<StudioSection>,
    pub bar_state: &'a TitleBarState,
}

/// Draws the frame. `scale` is the interface scale; the frame is not drawn in
/// full screen (it has no height then).
pub fn draw(ui: &mut Ui, model: &Model, chrome: &Chrome, parts: Parts<'_>, scale: f32, dt: f32) {
    let scale = scale.max(0.1);
    let (bar_px, rail_px) = (chrome.bar_height(), chrome.rail_width());
    let top_px = bar_px + chrome.nav_height();
    if bar_px > 0.0 {
        // The tabs and the status, under the title bar.
        ui.set_inset_left(0.0);
        ui.set_inset_top(bar_px / scale);
        parts.navbar.frame(ui, model, parts.nav_pointer);
        ui.set_inset_top(top_px / scale);
        // Only with the panel open: the viewer has no rail.
        if rail_px > 0.0 {
            // It slides in from the left, cut off at the edge of the room it
            // has taken so far.
            let slide = RAIL_WIDTH * (1.0 - chrome.rail_out.clamp(0.0, 1.0));
            let (_, rail_h) = ui.logical_size();
            ui.set_clip(Some(dockview_ui::Rect::new(
                0.0,
                0.0,
                rail_px / scale,
                rail_h,
            )));
            ui.set_offset(-slide, 0.0);
            parts
                .rail
                .frame(ui, model, parts.rail_pointer, parts.rail_active, dt);
            ui.set_offset(0.0, 0.0);
            ui.set_clip(None);
        }
        ui.set_inset_left(rail_px / scale);
        parts.strip.frame(ui, model);
        ui.set_inset_left(0.0);
    }
    // The window's own title bar, over everything.
    ui.set_inset_top(0.0);
    if bar_px > 0.0 {
        dockview_ui::titlebar::draw(
            ui,
            &model.theme,
            model.lang,
            parts.bar_state,
            &dockview_domain::brand::version_label(crate::os::renderer_name()),
        );
    }
}
