//! What goes inside a section once it is open: each section's own content.

use dockview_domain::stats::Stats;
use dockview_domain::view::Model;

use super::sections::StudioSection;
use super::StudioPanel;
use crate::Rect;

impl StudioPanel {
    /// Draws the body of `section` with its top-left corner at the corner of
    /// `body`. The caller has set the clip (so a half-open body shows only its
    /// top) and the opacity.
    pub(super) fn draw_body(
        &mut self,
        ui: &mut crate::Ui,
        m: &Model,
        stats: &Stats,
        section: StudioSection,
        body: Rect,
    ) {
        match section {
            StudioSection::Settings => self.draw_settings(ui, m, body),
            StudioSection::Sources => self.draw_sources(ui, m, body),
            StudioSection::Audio => self.draw_audio(ui, m, body),
            StudioSection::Scenes => self.draw_scenes(ui, m, body),
            StudioSection::Recording => self.draw_recording(ui, m, body),
            StudioSection::Stats => self.draw_stats(ui, m, stats, body),
        }
    }
}
