//! Fitting text into a width.

use super::{TextStyle, Ui};

/// Shortens `text` with an ellipsis so it fits in `max_w` logical pixels.
pub(crate) fn fit_text(ui: &mut Ui, text: &str, style: TextStyle, max_w: f32) -> String {
    if ui.measure(text, style).0 <= max_w {
        return text.to_owned();
    }
    let chars: Vec<char> = text.chars().collect();
    for keep in (1..chars.len()).rev() {
        let candidate: String = chars[..keep]
            .iter()
            .collect::<String>()
            .trim_end()
            .to_owned()
            + "…";
        if ui.measure(&candidate, style).0 <= max_w {
            return candidate;
        }
    }
    "…".to_owned()
}
