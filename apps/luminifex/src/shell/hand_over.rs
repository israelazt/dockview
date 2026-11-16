//! What the interface asked for in a frame, handed to the application.

use super::*;

impl Shell {
    /// Takes the actions every piece of the interface collected in the frame
    /// and gives them to the core, in the order the pieces are on screen.
    pub(super) fn hand_over_actions(&mut self) {
        let actions = self.controls.take_actions();
        if let Some(section) = self.rail.take_studio_request() {
            self.studio.open_section(section);
            self.side.open_section(section);
        }
        let editor = self.scene_editor.take_actions();
        for action in actions
            .into_iter()
            .chain(self.hud.take_actions())
            .chain(self.canvas_bar.take_actions())
            .chain(self.settings_page.take_actions())
            .chain(self.navbar.take_actions())
            .chain(self.rail.take_actions())
            .chain(self.studio.take_actions())
            .chain(self.side.take_actions())
            .chain(editor)
        {
            if action == dockview_domain::view::Action::OpenStats {
                // The Studio opens on its statistics.
                self.studio.open_section(StudioSection::Stats);
                self.side.open_section(StudioSection::Stats);
            }
            self.core.apply(action);
        }
    }
}
