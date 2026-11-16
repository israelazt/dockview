//! A line of text being typed: the text, where the caret is, and what the
//! keys do to them. Pure, so it is tested without a window.

/// A key that matters to a text field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextKey {
    /// What the key types (one or more characters).
    Text(String),
    Backspace,
    Delete,
    Left,
    Right,
    Home,
    End,
    /// Accept what was typed.
    Enter,
    /// Give up.
    Escape,
}

/// What a line of text being typed is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditTarget {
    /// The name of the scene at this position.
    Scene(usize),
    /// The address of the stream server.
    StreamUrl,
    /// The stream key (it is never shown).
    StreamKey,
}

/// What a key did, beyond changing the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Editing,
    Accepted,
    Cancelled,
}

/// The text and the caret (a position in characters, from 0 to its length).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    text: String,
    caret: usize,
}

impl TextEdit {
    /// Starts on `text` with the caret at its end.
    pub fn new(text: &str) -> Self {
        Self {
            text: text.to_owned(),
            caret: text.chars().count(),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// The text before the caret.
    pub fn before_caret(&self) -> &str {
        &self.text[..self.byte_of(self.caret)]
    }

    fn len(&self) -> usize {
        self.text.chars().count()
    }

    fn byte_of(&self, chars: usize) -> usize {
        self.text
            .char_indices()
            .nth(chars)
            .map_or(self.text.len(), |(i, _)| i)
    }

    /// Applies `key`. `accepts` says whether a longer text still fits (the
    /// field is only so wide, and a name only so long); typing that would not
    /// fit is dropped.
    pub fn apply(&mut self, key: &TextKey, accepts: &mut dyn FnMut(&str) -> bool) -> Outcome {
        match key {
            TextKey::Text(typed) => {
                // Line breaks and other controls are not text.
                for c in typed.chars().filter(|c| !c.is_control()) {
                    let at = self.byte_of(self.caret);
                    let mut next = self.text.clone();
                    next.insert(at, c);
                    if accepts(&next) {
                        self.text = next;
                        self.caret += 1;
                    }
                }
            }
            TextKey::Backspace if self.caret > 0 => {
                let (from, to) = (self.byte_of(self.caret - 1), self.byte_of(self.caret));
                self.text.replace_range(from..to, "");
                self.caret -= 1;
            }
            TextKey::Delete if self.caret < self.len() => {
                let (from, to) = (self.byte_of(self.caret), self.byte_of(self.caret + 1));
                self.text.replace_range(from..to, "");
            }
            TextKey::Left => self.caret = self.caret.saturating_sub(1),
            TextKey::Right => self.caret = (self.caret + 1).min(self.len()),
            TextKey::Home => self.caret = 0,
            TextKey::End => self.caret = self.len(),
            TextKey::Enter => return Outcome::Accepted,
            TextKey::Escape => return Outcome::Cancelled,
            TextKey::Backspace | TextKey::Delete => {}
        }
        Outcome::Editing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(edit: &mut TextEdit, text: &str) {
        edit.apply(&TextKey::Text(text.into()), &mut |_| true);
    }

    #[test]
    fn typing_goes_at_the_caret_which_starts_at_the_end() {
        let mut e = TextEdit::new("Sce");
        typed(&mut e, "ne");
        assert_eq!(e.text(), "Scene");
        e.apply(&TextKey::Home, &mut |_| true);
        typed(&mut e, ">");
        assert_eq!((e.text(), e.before_caret()), (">Scene", ">"));
    }

    #[test]
    fn backspace_and_delete_remove_one_character_either_side() {
        let mut e = TextEdit::new("abc");
        e.apply(&TextKey::Left, &mut |_| true);
        e.apply(&TextKey::Backspace, &mut |_| true);
        assert_eq!(e.text(), "ac");
        e.apply(&TextKey::Delete, &mut |_| true);
        assert_eq!(e.text(), "a");
        // At the ends they do nothing.
        e.apply(&TextKey::Delete, &mut |_| true);
        e.apply(&TextKey::Home, &mut |_| true);
        e.apply(&TextKey::Backspace, &mut |_| true);
        assert_eq!(e.text(), "a");
    }

    #[test]
    fn accented_and_wide_characters_are_one_step_each() {
        let mut e = TextEdit::new("añá");
        e.apply(&TextKey::Left, &mut |_| true);
        e.apply(&TextKey::Backspace, &mut |_| true);
        assert_eq!(e.text(), "aá");
        e.apply(&TextKey::End, &mut |_| true);
        typed(&mut e, "😀");
        assert_eq!(e.text(), "aá😀");
        e.apply(&TextKey::Backspace, &mut |_| true);
        assert_eq!(e.text(), "aá");
    }

    #[test]
    fn the_caret_stays_inside_the_text() {
        let mut e = TextEdit::new("ab");
        for _ in 0..5 {
            e.apply(&TextKey::Right, &mut |_| true);
        }
        assert_eq!(e.before_caret(), "ab");
        for _ in 0..5 {
            e.apply(&TextKey::Left, &mut |_| true);
        }
        assert_eq!(e.before_caret(), "");
    }

    #[test]
    fn typing_that_does_not_fit_is_dropped_and_controls_are_not_text() {
        let mut e = TextEdit::new("ab");
        let mut short = |t: &str| t.chars().count() <= 3;
        e.apply(&TextKey::Text("cde".into()), &mut short);
        assert_eq!(e.text(), "abc");
        e.apply(&TextKey::Backspace, &mut short);
        e.apply(&TextKey::Text("\n\t".into()), &mut |_| true);
        assert_eq!(e.text(), "ab", "line breaks and tabs are not text");
    }

    #[test]
    fn enter_accepts_and_escape_cancels() {
        let mut e = TextEdit::new("x");
        assert_eq!(e.apply(&TextKey::Enter, &mut |_| true), Outcome::Accepted);
        assert_eq!(e.apply(&TextKey::Escape, &mut |_| true), Outcome::Cancelled);
        assert_eq!(e.apply(&TextKey::Left, &mut |_| true), Outcome::Editing);
    }
}
