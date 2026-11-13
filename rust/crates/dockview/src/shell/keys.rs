//! Which key asks for what.

use dockview_app::keys::KeyCommand;
use winit::keyboard::{Key, NamedKey};

/// The command for `key`, if it has one. Letters ignore case.
pub fn command_for(key: &Key) -> Option<KeyCommand> {
    match key {
        Key::Named(NamedKey::F11) => Some(KeyCommand::ToggleFullscreen),
        Key::Named(NamedKey::Escape) => Some(KeyCommand::Escape),
        Key::Named(NamedKey::ArrowUp) => Some(KeyCommand::VolumeUp),
        Key::Named(NamedKey::ArrowDown) => Some(KeyCommand::VolumeDown),
        Key::Named(NamedKey::ArrowLeft) => Some(KeyCommand::SyncEarlier),
        Key::Named(NamedKey::ArrowRight) => Some(KeyCommand::SyncLater),
        Key::Character(c) => match c.to_ascii_lowercase().as_str() {
            "f" => Some(KeyCommand::ToggleFullscreen),
            "p" => Some(KeyCommand::CyclePreset),
            "i" => Some(KeyCommand::ToggleStats),
            "v" => Some(KeyCommand::NextVideo),
            "a" => Some(KeyCommand::NextAudio),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ch(text: &str) -> Key {
        Key::Character(text.into())
    }

    #[test]
    fn the_named_keys_do_what_the_header_says() {
        let named = |k| command_for(&Key::Named(k));
        assert_eq!(named(NamedKey::F11), Some(KeyCommand::ToggleFullscreen));
        assert_eq!(named(NamedKey::Escape), Some(KeyCommand::Escape));
        assert_eq!(named(NamedKey::ArrowUp), Some(KeyCommand::VolumeUp));
        assert_eq!(named(NamedKey::ArrowDown), Some(KeyCommand::VolumeDown));
        assert_eq!(named(NamedKey::ArrowLeft), Some(KeyCommand::SyncEarlier));
        assert_eq!(named(NamedKey::ArrowRight), Some(KeyCommand::SyncLater));
        assert_eq!(named(NamedKey::Enter), None);
    }

    #[test]
    fn letters_work_in_either_case() {
        for (text, command) in [
            ("f", KeyCommand::ToggleFullscreen),
            ("p", KeyCommand::CyclePreset),
            ("i", KeyCommand::ToggleStats),
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
