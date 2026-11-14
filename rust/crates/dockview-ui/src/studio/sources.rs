//! The Sources section: the capture cards, the screens and the windows that
//! can be shown, with the same choices (and actions) as the source popover.

use dockview_domain::i18n::{tr, Key};
use dockview_domain::view::{Action, CurrentSource, Model};

use super::widgets::{list_height, GAP, LABEL_H};
use super::{StudioPanel, PAD};
use crate::Rect;

/// Most cards or screens listed, and most windows (the frontmost ones).
const MAX_DEVICES: usize = 5;
const MAX_WINDOWS: usize = 8;
const PAD_TOP: f32 = 4.0;
const PAD_BOTTOM: f32 = 14.0;

/// How tall the body is: three titled lists, as long as what there is to list.
pub fn body_height(m: &Model) -> f32 {
    let group = |count: usize, limit: usize| LABEL_H + list_height(count, limit);
    PAD_TOP
        + group(m.video_devices.len(), MAX_DEVICES)
        + group(m.sources.monitors.len(), MAX_DEVICES)
        + group(m.sources.windows.len(), MAX_WINDOWS)
        + GAP * 2.0
        + PAD_BOTTOM
}

impl StudioPanel {
    /// Draws the body of the Sources section with its top-left corner at the
    /// corner of `body`.
    pub(super) fn draw_sources(&mut self, ui: &mut crate::Ui, m: &Model, body: Rect) {
        let t = &m.theme;
        let lang = m.lang;
        let (x, w) = (body.x + PAD, body.w - PAD * 2.0);
        let mut y = body.y + PAD_TOP;
        let current = m.sources.current;
        let card = (current == CurrentSource::Capture)
            .then_some(m.video_selected)
            .flatten();
        let screen = match current {
            CurrentSource::Monitor(i) => Some(i),
            _ => None,
        };
        let window = match current {
            CurrentSource::Window(i) => i,
            _ => None,
        };

        y += self.caption(ui, t, tr(lang, Key::SourceCapture), x, y);
        y += self.list(
            ui,
            t,
            lang,
            &m.video_devices,
            card,
            MAX_DEVICES,
            &Action::SelectVideo,
            x,
            y,
            w,
        ) + GAP;
        y += self.caption(ui, t, tr(lang, Key::SourceScreen), x, y);
        y += self.list(
            ui,
            t,
            lang,
            &m.sources.monitors,
            screen,
            MAX_DEVICES,
            &Action::SelectMonitor,
            x,
            y,
            w,
        ) + GAP;
        y += self.caption(ui, t, tr(lang, Key::SourceApp), x, y);
        self.list(
            ui,
            t,
            lang,
            &m.sources.windows,
            window,
            MAX_WINDOWS,
            &Action::SelectWindow,
            x,
            y,
            w,
        );
    }
}
