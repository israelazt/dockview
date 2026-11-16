//! Which key asks for what.

use dockview_app::keys::KeyCommand;
use dockview_ui::studio::TextKey;
use winit::keyboard::{Key, NamedKey};

/// The command for `key`, if it has one. Letters ignore case.
#[cfg(test)]
pub fn command_for(key: &Key) -> Option<KeyCommand> {
    command_with(key, false)
}

/// Like [`command_for`], knowing whether Ctrl is held: Ctrl+Z undoes, and Ctrl
/// with other letters is not for us.
pub fn command_with(key: &Key, ctrl: bool) -> Option<KeyCommand> {
    if ctrl {
        return match key {
            Key::Character(c) if c.eq_ignore_ascii_case("z") => Some(KeyCommand::Undo),
            _ => None,
        };
    }
    match key {
        Key::Named(NamedKey::F11) => Some(KeyCommand::ToggleFullscreen),
        Key::Named(NamedKey::Escape) => Some(KeyCommand::Escape),
        Key::Named(NamedKey::Tab) => Some(KeyCommand::ToggleStudio),
        Key::Named(NamedKey::ArrowUp) => Some(KeyCommand::VolumeUp),
        Key::Named(NamedKey::ArrowDown) => Some(KeyCommand::VolumeDown),
        Key::Named(NamedKey::ArrowLeft) => Some(KeyCommand::SyncEarlier),
        Key::Named(NamedKey::ArrowRight) => Some(KeyCommand::SyncLater),
        Key::Character(c) => match c.to_ascii_lowercase().as_str() {
            "f" => Some(KeyCommand::ToggleFullscreen),
            "p" => Some(KeyCommand::CyclePreset),
            "h" => Some(KeyCommand::ToggleHud),
            "v" => Some(KeyCommand::NextVideo),
            "a" => Some(KeyCommand::NextAudio),
            _ => None,
        },
        _ => None,
    }
}

/// What `key` does in a text field, given the text the key types (the
/// keyboard layout and the modifiers have already been applied to it).
pub fn text_key_for(key: &Key, text: Option<&str>) -> Option<TextKey> {
    match key {
        Key::Named(NamedKey::Backspace) => Some(TextKey::Backspace),
        Key::Named(NamedKey::Delete) => Some(TextKey::Delete),
        Key::Named(NamedKey::ArrowLeft) => Some(TextKey::Left),
        Key::Named(NamedKey::ArrowRight) => Some(TextKey::Right),
        Key::Named(NamedKey::Home) => Some(TextKey::Home),
        Key::Named(NamedKey::End) => Some(TextKey::End),
        Key::Named(NamedKey::Enter) => Some(TextKey::Enter),
        Key::Named(NamedKey::Escape) => Some(TextKey::Escape),
        _ => text
            .filter(|t| t.chars().any(|c| !c.is_control()))
            .map(|t| TextKey::Text(t.to_owned())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_text_field_gets_editing_keys_and_typed_text_only() {
        let named = |k| text_key_for(&Key::Named(k), None);
        assert_eq!(named(NamedKey::Backspace), Some(TextKey::Backspace));
        assert_eq!(named(NamedKey::Delete), Some(TextKey::Delete));
        assert_eq!(named(NamedKey::ArrowLeft), Some(TextKey::Left));
        assert_eq!(named(NamedKey::ArrowRight), Some(TextKey::Right));
        assert_eq!(named(NamedKey::Home), Some(TextKey::Home));
        assert_eq!(named(NamedKey::End), Some(TextKey::End));
        assert_eq!(named(NamedKey::Enter), Some(TextKey::Enter));
        assert_eq!(named(NamedKey::Escape), Some(TextKey::Escape));
        // Letters, and the space bar, come with their text.
        assert_eq!(
            text_key_for(&Key::Character("ñ".into()), Some("ñ")),
            Some(TextKey::Text("ñ".into()))
        );
        assert_eq!(
            text_key_for(&Key::Named(NamedKey::Space), Some(" ")),
            Some(TextKey::Text(" ".into()))
        );
        // Tab types a control character, and F11 nothing: neither is text.
        assert_eq!(text_key_for(&Key::Named(NamedKey::Tab), Some("\t")), None);
        assert_eq!(text_key_for(&Key::Named(NamedKey::F11), None), None);
    }

    fn ch(text: &str) -> Key {
        Key::Character(text.into())
    }

    #[test]
    fn the_named_keys_do_what_the_header_says() {
        let named = |k| command_for(&Key::Named(k));
        assert_eq!(named(NamedKey::F11), Some(KeyCommand::ToggleFullscreen));
        assert_eq!(named(NamedKey::Escape), Some(KeyCommand::Escape));
        assert_eq!(named(NamedKey::Tab), Some(KeyCommand::ToggleStudio));
        assert_eq!(named(NamedKey::ArrowUp), Some(KeyCommand::VolumeUp));
        assert_eq!(named(NamedKey::ArrowDown), Some(KeyCommand::VolumeDown));
        assert_eq!(named(NamedKey::ArrowLeft), Some(KeyCommand::SyncEarlier));
        assert_eq!(named(NamedKey::ArrowRight), Some(KeyCommand::SyncLater));
        assert_eq!(named(NamedKey::Enter), None);
    }

    #[test]
    fn ctrl_z_undoes_and_other_ctrl_chords_are_left_alone() {
        assert_eq!(command_with(&ch("z"), true), Some(KeyCommand::Undo));
        assert_eq!(command_with(&ch("Z"), true), Some(KeyCommand::Undo));
        assert_eq!(
            command_with(&ch("f"), true),
            None,
            "Ctrl+F is not fullscreen"
        );
        assert_eq!(command_with(&ch("z"), false), None);
        assert_eq!(
            command_with(&Key::Named(NamedKey::F11), false),
            Some(KeyCommand::ToggleFullscreen)
        );
    }

    #[test]
    fn letters_work_in_either_case() {
        for (text, command) in [
            ("f", KeyCommand::ToggleFullscreen),
            ("p", KeyCommand::CyclePreset),
            ("h", KeyCommand::ToggleHud),
            ("v", KeyCommand::NextVideo),
            ("a", KeyCommand::NextAudio),
        ] {
            assert_eq!(command_for(&ch(text)), Some(command), "{text}");
            assert_eq!(command_for(&ch(&text.to_uppercase())), Some(command));
        }
    }

    #[test]
    fn other_keys_do_nothing() {
        assert_eq!(command_for(&ch("z")), None);
        assert_eq!(command_for(&ch("1")), None);
        assert_eq!(command_for(&Key::Dead(None)), None);
    }
}
