//! The "add a source" view of the scenes popover.

use super::*;

impl Controls {
    pub(super) fn draw_add_view(&mut self, ui: &mut Ui, m: &Model, x: f32, mut y: f32, w: f32) {
        let t = &m.theme;
        let row = self.scene_row(ui, m, SceneHit::Back, x, y, w);
        ui.icon(ICON_BACK, x + 14.0, y + ROW_HEIGHT / 2.0, 15.0, t.text_dim);
        self.row_label(
            ui,
            tr(m.lang, Key::SceneAddTitle),
            row,
            x + 32.0,
            10.0,
            t.text,
        );
        y += ROW_HEIGHT + 8.0;

        y = self.list_section(
            ui,
            m,
            Key::SourceCapture,
            &m.video_devices,
            None,
            |i| Hit::Scenes(SceneHit::AddCapture(i)),
            x,
            y,
            w,
            MAX_ADD_DEVICES,
        );
        y += SECTION_GAP;
        y = self.list_section(
            ui,
            m,
            Key::SourceScreen,
            &m.sources.monitors,
            None,
            |i| Hit::Scenes(SceneHit::AddMonitor(i)),
            x,
            y,
            w,
            MAX_ADD_DEVICES,
        );
        y += SECTION_GAP;
        self.list_section(
            ui,
            m,
            Key::SourceApp,
            &m.sources.windows,
            None,
            |i| Hit::Scenes(SceneHit::AddWindow(i)),
            x,
            y,
            w,
            MAX_ADD_WINDOWS,
        );
    }
}
