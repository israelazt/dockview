//! A global hotkey as the user writes and saves it ("PrintScreen",
//! "Ctrl+Shift+S"): parsing, writing and the rules for which combinations are
//! allowed. Which key code that is on a given system is the platform's
//! business; this is the same everywhere.

use std::fmt;

use serde::{Deserialize, Serialize};

/// The keys a global hotkey may use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// `A` to `Z`.
    Letter(char),
    /// `0` to `9`.
    Digit(u8),
    /// `F1` to `F24`.
    Function(u8),
    PrintScreen,
    Pause,
    ScrollLock,
    Insert,
    Delete,
    Home,
    End,
    PageUp,
    PageDown,
}

impl Key {
    /// `F1`, `PrintScreen`, `S`...
    fn parse(text: &str) -> Option<Self> {
        let upper = text.trim().to_ascii_uppercase();
        let named = match upper.as_str() {
            "PRINTSCREEN" | "PRTSC" | "PRTSCN" | "IMPRPANT" => Some(Self::PrintScreen),
            "PAUSE" => Some(Self::Pause),
            "SCROLLLOCK" => Some(Self::ScrollLock),
            "INSERT" | "INS" => Some(Self::Insert),
            "DELETE" | "DEL" => Some(Self::Delete),
            "HOME" => Some(Self::Home),
            "END" => Some(Self::End),
            "PAGEUP" | "PGUP" => Some(Self::PageUp),
            "PAGEDOWN" | "PGDN" => Some(Self::PageDown),
            _ => None,
        };
        if named.is_some() {
            return named;
        }
        let mut chars = upper.chars();
        match (chars.next(), chars.as_str()) {
            (Some(c @ 'A'..='Z'), "") => Some(Self::Letter(c)),
            (Some(c @ '0'..='9'), "") => Some(Self::Digit(c as u8 - b'0')),
            (Some('F'), n) => n
                .parse::<u8>()
                .ok()
                .filter(|n| (1..=24).contains(n))
                .map(Self::Function),
            _ => None,
        }
    }

    /// Whether the key types something on its own (so it needs a modifier to
    /// be taken away from the program in front).
    fn types_text(self) -> bool {
        matches!(
            self,
            Key::Letter(_)
                | Key::Digit(_)
                | Key::Insert
                | Key::Delete
                | Key::Home
                | Key::End
                | Key::PageUp
                | Key::PageDown
        )
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Key::Letter(c) => write!(f, "{c}"),
            Key::Digit(d) => write!(f, "{d}"),
            Key::Function(n) => write!(f, "F{n}"),
            Key::PrintScreen => f.write_str("PrintScreen"),
            Key::Pause => f.write_str("Pause"),
            Key::ScrollLock => f.write_str("ScrollLock"),
            Key::Insert => f.write_str("Insert"),
            Key::Delete => f.write_str("Delete"),
            Key::Home => f.write_str("Home"),
            Key::End => f.write_str("End"),
            Key::PageUp => f.write_str("PageUp"),
            Key::PageDown => f.write_str("PageDown"),
        }
    }
}

/// Why a combination cannot be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyError {
    /// A letter, digit or editing key alone would take it away from whatever
    /// the user is typing in.
    NeedsModifier,
    /// Ctrl+Alt is AltGr on many keyboards: with a letter or digit it would
    /// steal characters such as `@` or `|`.
    AltGr,
    /// Dockview already uses it (the replay and scene hotkeys).
    Reserved,
}

/// Modifiers and a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct HotkeySpec {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// The Windows (or Super) key.
    pub win: bool,
    pub key: Key,
}

impl HotkeySpec {
    /// The key on its own.
    pub const fn key(key: Key) -> Self {
        Self {
            ctrl: false,
            alt: false,
            shift: false,
            win: false,
            key,
        }
    }

    /// What the capture hotkey is until the user changes it.
    pub const DEFAULT_CAPTURE: HotkeySpec = HotkeySpec::key(Key::PrintScreen);

    /// `Ctrl+Shift+S`, `PrintScreen`... (any case, `+` between the parts).
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut spec = Self::key(Key::PrintScreen);
        let mut key = None;
        for part in text.split('+') {
            match part.trim().to_ascii_uppercase().as_str() {
                "CTRL" | "CONTROL" => spec.ctrl = true,
                "ALT" => spec.alt = true,
                "SHIFT" => spec.shift = true,
                "WIN" | "WINDOWS" | "SUPER" | "META" => spec.win = true,
                other => {
                    if key.is_some() {
                        return Err(format!("'{text}': more than one key"));
                    }
                    key = Some(
                        Key::parse(other)
                            .ok_or_else(|| format!("'{other}' is not a key I know"))?,
                    );
                }
            }
        }
        spec.key = key.ok_or_else(|| format!("'{text}': no key"))?;
        Ok(spec)
    }

    /// Whether this combination may be used, and why not.
    pub fn check(&self) -> Result<(), HotkeyError> {
        if self.is_reserved() {
            return Err(HotkeyError::Reserved);
        }
        let has_real_modifier = self.ctrl || self.alt || self.win;
        if self.key.types_text() && !has_real_modifier {
            return Err(HotkeyError::NeedsModifier);
        }
        if self.ctrl && self.alt && !self.win && matches!(self.key, Key::Letter(_) | Key::Digit(_))
        {
            return Err(HotkeyError::AltGr);
        }
        Ok(())
    }

    /// The combinations Dockview itself registers: Ctrl+Alt+S and R (the replay
    /// and recording) and Ctrl+Alt+F1 to F9 and F12 (the scenes).
    pub fn is_reserved(&self) -> bool {
        if !(self.ctrl && self.alt && !self.shift && !self.win) {
            return false;
        }
        matches!(self.key, Key::Letter('S' | 'R') | Key::Function(1..=9 | 12))
    }
}

impl fmt::Display for HotkeySpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (on, name) in [
            (self.ctrl, "Ctrl"),
            (self.alt, "Alt"),
            (self.shift, "Shift"),
            (self.win, "Win"),
        ] {
            if on {
                write!(f, "{name}+")?;
            }
        }
        write!(f, "{}", self.key)
    }
}

impl From<HotkeySpec> for String {
    fn from(spec: HotkeySpec) -> Self {
        spec.to_string()
    }
}

impl TryFrom<String> for HotkeySpec {
    type Error = String;
    fn try_from(text: String) -> Result<Self, String> {
        Self::parse(&text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(text: &str) -> HotkeySpec {
        HotkeySpec::parse(text).unwrap()
    }

    #[test]
    fn a_hotkey_is_read_in_any_case_and_written_the_same_way() {
        assert_eq!(spec("printscreen"), HotkeySpec::key(Key::PrintScreen));
        let s = spec("ctrl+SHIFT+s");
        assert!(s.ctrl && s.shift && !s.alt && !s.win);
        assert_eq!(s.key, Key::Letter('S'));
        assert_eq!(s.to_string(), "Ctrl+Shift+S");
        assert_eq!(spec("Win + Shift + S").to_string(), "Shift+Win+S");
        for text in [
            "PrintScreen",
            "Ctrl+F9",
            "Alt+Shift+Pause",
            "Ctrl+Alt+PageDown",
            "Ctrl+7",
            "Shift+Win+S",
        ] {
            assert_eq!(spec(text).to_string(), text, "{text}");
        }
    }

    #[test]
    fn nonsense_is_an_error() {
        for bad in [
            "",
            "Ctrl+",
            "Ctrl+Nothing",
            "F25",
            "F0",
            "A+B",
            "Ctrl+Alt",
            "+S",
        ] {
            assert!(HotkeySpec::parse(bad).is_err(), "{bad:?}");
        }
        assert_eq!(spec("F24").key, Key::Function(24));
        assert_eq!(spec("prtsc").key, Key::PrintScreen);
    }

    #[test]
    fn keys_that_type_need_a_real_modifier_and_function_keys_do_not() {
        assert_eq!(spec("S").check(), Err(HotkeyError::NeedsModifier));
        assert_eq!(
            spec("Shift+S").check(),
            Err(HotkeyError::NeedsModifier),
            "Shift types a capital"
        );
        assert_eq!(spec("Delete").check(), Err(HotkeyError::NeedsModifier));
        assert!(spec("Ctrl+S").check().is_ok());
        assert!(spec("Win+Shift+S").check().is_ok());
        for alone in ["PrintScreen", "F9", "Pause", "ScrollLock", "F13"] {
            assert!(spec(alone).check().is_ok(), "{alone}");
        }
    }

    #[test]
    fn ctrl_alt_with_a_letter_or_digit_is_refused_for_altgr_but_not_with_a_function_key() {
        assert_eq!(spec("Ctrl+Alt+Q").check(), Err(HotkeyError::AltGr));
        assert_eq!(spec("Ctrl+Alt+2").check(), Err(HotkeyError::AltGr));
        assert!(spec("Ctrl+Alt+F10").check().is_ok());
        assert!(spec("Ctrl+Alt+Win+Q").check().is_ok());
    }

    #[test]
    fn what_dockview_already_uses_is_reserved() {
        for taken in [
            "Ctrl+Alt+S",
            "Ctrl+Alt+R",
            "Ctrl+Alt+F1",
            "Ctrl+Alt+F9",
            "Ctrl+Alt+F12",
        ] {
            assert!(spec(taken).is_reserved(), "{taken}");
            assert_eq!(spec(taken).check(), Err(HotkeyError::Reserved));
        }
        assert!(!spec("Ctrl+Alt+F10").is_reserved());
        assert!(!spec("Ctrl+Shift+S").is_reserved());
    }

    #[test]
    fn it_is_saved_as_text_and_a_bad_value_in_the_file_is_an_error_not_a_panic() {
        let json = serde_json::to_string(&spec("Ctrl+Shift+S")).unwrap();
        assert_eq!(json, r#""Ctrl+Shift+S""#);
        assert_eq!(
            serde_json::from_str::<HotkeySpec>(&json).unwrap(),
            spec("Ctrl+Shift+S")
        );
        assert!(serde_json::from_str::<HotkeySpec>(r#""Banana""#).is_err());
        assert_eq!(HotkeySpec::DEFAULT_CAPTURE.to_string(), "PrintScreen");
        assert!(HotkeySpec::DEFAULT_CAPTURE.check().is_ok());
    }
}
