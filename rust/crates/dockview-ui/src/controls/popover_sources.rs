//! The source popover: capture cards, monitors and application windows.

use super::*;

impl Controls {
    /// The source popover: capture cards, monitors and application windows.
    pub(super) fn draw_sources_popover(&mut self, ui: &mut Ui, m: &Model, pill: Rect) {
        let t = &m.theme;
        let height = sources_height(m);
        let (screen_w, _) = ui.logical_size();
        let x = (pill.x + pill.w - POPOVER_WIDTH)
            .min(screen_w - POPOVER_WIDTH - POPOVER_GAP)
            .max(POPOVER_GAP)
            .round();
        let y = (pill.y - POPOVER_GAP - height).max(POPOVER_GAP).round();
        let rect = Rect::new(x, y, POPOVER_WIDTH, height);
        self.register(Hit::Popover, rect);
        ui.shadow(rect, 20.0, 26.0, t.shadow);
        ui.panel(rect, 20.0, t.surface, t.border, 1.0);

        let inner_x = x + POPOVER_PAD;
        let inner_w = POPOVER_WIDTH - POPOVER_PAD * 2.0;
        let mut y = y + POPOVER_PAD;
        let current = m.sources.current;
        let capture_selected = (current == CurrentSource::Capture)
            .then_some(m.video_selected)
            .flatten();
        let monitor_selected = match current {
            CurrentSource::Monitor(i) => Some(i),
            _ => None,
        };
        let window_selected = match current {
            CurrentSource::Window(i) => i,
            _ => None,
        };
        y = self.list_section(
            ui,
            m,
            Key::SourceCapture,
            &m.video_devices,
            capture_selected,
            Hit::Video,
            inner_x,
            y,
            inner_w,
            MAX_LISTED_DEVICES,
        );
        y += SECTION_GAP;
        y = self.list_section(
            ui,
            m,
            Key::SourceScreen,
            &m.sources.monitors,
            monitor_selected,
            Hit::SourceMonitor,
            inner_x,
            y,
            inner_w,
            MAX_LISTED_DEVICES,
        );
        y += SECTION_GAP;
        self.list_section(
            ui,
            m,
            Key::SourceApp,
            &m.sources.windows,
            window_selected,
            Hit::SourceWindow,
            inner_x,
            y,
            inner_w,
            MAX_LISTED_WINDOWS,
        );
    }
}

/// Height of the source popover for `m`.
pub(super) fn sources_height(m: &Model) -> f32 {
    let section = |n: usize, limit: usize| LABEL_HEIGHT + n.clamp(1, limit) as f32 * ROW_HEIGHT;
    POPOVER_PAD * 2.0
        + section(m.video_devices.len(), MAX_LISTED_DEVICES)
        + section(m.sources.monitors.len(), MAX_LISTED_DEVICES)
        + section(m.sources.windows.len(), MAX_LISTED_WINDOWS)
        + SECTION_GAP * 2.0
}
