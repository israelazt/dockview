//! Typing into the panel: the keys the window hands over while a name is being
//! written, and what they do to it.

use dockview_domain::view::Action;

use super::{edit, scenes, StudioPanel, TextKey};
use crate::Ui;

/// The longest an address or a key may be typed.
const MAX_STREAM_TEXT_CHARS: usize = 300;

impl StudioPanel {
    /// Whether a name is being typed (the keys then go to the field and not to
    /// the shortcuts).
    pub fn is_editing(&self) -> bool {
        self.editing.is_some()
    }

    /// A key for the field being typed in.
    pub fn key(&mut self, key: TextKey) {
        if self.editing.is_some() {
            self.typed.push(key);
        }
    }

    /// Applies the keys typed since the last frame to the name being typed.
    pub(super) fn apply_typed_keys(&mut self, ui: &mut Ui) {
        let keys = std::mem::take(&mut self.typed);
        let room = self.edit_room;
        for key in keys {
            let Some((index, edit)) = self.editing.as_mut() else {
                return;
            };
            let target = *index;
            let mut fits = |text: &str| match target {
                edit::EditTarget::Scene(_) => {
                    text.chars().count() <= dockview_domain::scene::MAX_SCENE_NAME_CHARS
                        && ui.measure(text, scenes::ROW_TEXT).0 <= room
                }
                // An address or a key is long; the field shows its end.
                edit::EditTarget::StreamUrl | edit::EditTarget::StreamKey => {
                    text.chars().count() <= MAX_STREAM_TEXT_CHARS
                }
            };
            match edit.apply(&key, &mut fits) {
                edit::Outcome::Editing => {}
                edit::Outcome::Accepted => {
                    let text = edit.text().to_owned();
                    let action = match target {
                        edit::EditTarget::Scene(i) => Action::RenameScene(i, text),
                        edit::EditTarget::StreamUrl => Action::SetStreamUrl(text),
                        edit::EditTarget::StreamKey => Action::SetStreamKey(text),
                    };
                    self.actions.push(action);
                    self.editing = None;
                }
                edit::Outcome::Cancelled => self.editing = None,
            }
        }
    }
}
