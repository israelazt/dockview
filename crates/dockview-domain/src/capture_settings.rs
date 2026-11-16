//! What the user chooses about screenshots, saved with the rest of the
//! settings, and how a choice becomes a [`SnapJob`].

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::hotkey::HotkeySpec;
use crate::image::{screenshot_name, ImageFormat};
use crate::select::Mode;
use crate::snap::{SnapJob, SnapTarget, DELAYS, MAX_DELAY_SECS};

/// Screenshot settings. Every field falls back to its default when missing from
/// the file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CaptureSettings {
    /// The global hotkey that starts a capture; `None` is no hotkey (the tray
    /// icon and the button still work).
    #[serde(deserialize_with = "lenient_hotkey")]
    pub hotkey: Option<HotkeySpec>,
    /// Save the picture to a file.
    pub save: bool,
    /// Where pictures are saved; `None` is Pictures\\Screenshots.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    pub format: ImageFormat,
    /// Put the picture on the clipboard.
    pub copy: bool,
    /// Draw the mouse cursor in the picture.
    pub cursor: bool,
    /// Seconds to wait before the screen freezes (0, 3, 5 or 10).
    pub delay_secs: u32,
    /// How the area is chosen when a capture starts.
    pub mode: Mode,
    /// Closing the window leaves Dockview in the tray (so the hotkey keeps
    /// working) instead of quitting.
    pub keep_in_tray: bool,
}

/// Reads the hotkey, and falls back to the default if the text is not one: a
/// typo in the file must not make the whole settings file unreadable.
fn lenient_hotkey<'de, D>(deserializer: D) -> Result<Option<HotkeySpec>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let text = Option::<String>::deserialize(deserializer)?;
    Ok(text.map(|text| HotkeySpec::parse(&text).unwrap_or(HotkeySpec::DEFAULT_CAPTURE)))
}

impl Default for CaptureSettings {
    fn default() -> Self {
        Self {
            hotkey: Some(HotkeySpec::DEFAULT_CAPTURE),
            save: true,
            folder: None,
            format: ImageFormat::Png,
            copy: true,
            cursor: false,
            delay_secs: 0,
            mode: Mode::Rectangle,
            keep_in_tray: true,
        }
    }
}

impl CaptureSettings {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// Values from an old or hand-edited file, brought back to sensible ones.
    #[must_use]
    pub fn sanitized(mut self) -> Self {
        if !DELAYS.contains(&self.delay_secs) {
            self.delay_secs = self.delay_secs.min(MAX_DELAY_SECS);
        }
        // A hotkey that cannot be used is dropped, not kept to fail at start-up.
        if self.hotkey.is_some_and(|h| h.check().is_err()) {
            self.hotkey = Some(HotkeySpec::DEFAULT_CAPTURE);
        }
        self
    }

    /// The screenshot to take now: in the mode chosen (or `mode`), saved in
    /// `default_folder` (or the folder chosen) with the name of the moment
    /// `(year, month, day)` `(hour, minute, second)`.
    pub fn job(
        &self,
        mode: Mode,
        default_folder: &Path,
        date: (u16, u16, u16),
        time: (u16, u16, u16),
    ) -> SnapJob {
        let folder = self.folder.as_deref().map_or(default_folder, Path::new);
        SnapJob {
            target: SnapTarget::Select(mode),
            out: self
                .save
                .then(|| folder.join(screenshot_name(date, time, self.format))),
            cursor: self.cursor,
            copy: self.copy,
            delay_secs: self.delay_secs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DATE: (u16, u16, u16) = (2026, 11, 14);
    const TIME: (u16, u16, u16) = (16, 30, 12);

    #[test]
    fn the_defaults_are_print_screen_save_and_copy_in_a_rectangle() {
        let s = CaptureSettings::default();
        assert_eq!(s.hotkey, Some(HotkeySpec::DEFAULT_CAPTURE));
        assert!(s.save && s.copy && s.keep_in_tray && !s.cursor);
        assert_eq!(
            (s.format, s.mode, s.delay_secs),
            (ImageFormat::Png, Mode::Rectangle, 0)
        );
        assert!(s.is_default());
    }

    #[test]
    fn a_job_saves_into_the_default_folder_with_the_name_of_the_moment() {
        let job = CaptureSettings::default().job(
            Mode::Window,
            Path::new("Pictures/Screenshots"),
            DATE,
            TIME,
        );
        assert_eq!(job.target, SnapTarget::Select(Mode::Window));
        assert_eq!(
            job.out.as_deref(),
            Some(Path::new(
                "Pictures/Screenshots/Screenshot 2026-11-14 163012.png"
            ))
        );
        assert!(job.copy && !job.cursor);
    }

    #[test]
    fn the_folder_and_format_chosen_win_and_save_off_means_no_file() {
        let s = CaptureSettings {
            folder: Some("D:/shots".into()),
            format: ImageFormat::Jpeg,
            ..CaptureSettings::default()
        };
        let job = s.job(Mode::Rectangle, Path::new("unused"), DATE, TIME);
        assert_eq!(
            job.out.as_deref(),
            Some(Path::new("D:/shots/Screenshot 2026-11-14 163012.jpg"))
        );
        let no_file = CaptureSettings {
            save: false,
            ..CaptureSettings::default()
        };
        assert_eq!(
            no_file.job(Mode::Full, Path::new("x"), DATE, TIME).out,
            None
        );
    }

    #[test]
    fn the_delay_and_the_cursor_travel_with_the_job() {
        let s = CaptureSettings {
            delay_secs: 5,
            cursor: true,
            ..CaptureSettings::default()
        };
        let job = s.job(Mode::Rectangle, Path::new("x"), DATE, TIME);
        assert_eq!((job.delay_secs, job.cursor), (5, true));
    }

    #[test]
    fn an_old_file_loads_and_odd_values_are_brought_back() {
        let from_empty: CaptureSettings = serde_json::from_str("{}").unwrap();
        assert!(from_empty.is_default());
        let odd: CaptureSettings =
            serde_json::from_str(r#"{"hotkey":"S","delaySecs":900,"copy":false}"#).unwrap();
        let fixed = odd.sanitized();
        assert_eq!(
            fixed.hotkey,
            Some(HotkeySpec::DEFAULT_CAPTURE),
            "a bare letter cannot be a hotkey"
        );
        assert_eq!(fixed.delay_secs, MAX_DELAY_SECS);
        assert!(!fixed.copy);
        // Text that is not a hotkey does not break the file either.
        let typo: CaptureSettings = serde_json::from_str(r#"{"hotkey":"Banana"}"#).unwrap();
        assert_eq!(typo.hotkey, Some(HotkeySpec::DEFAULT_CAPTURE));
        // No hotkey at all is a valid choice.
        let none: CaptureSettings = serde_json::from_str(r#"{"hotkey":null}"#).unwrap();
        assert_eq!(none.sanitized().hotkey, None);
    }

    #[test]
    fn it_survives_a_round_trip() {
        let s = CaptureSettings {
            hotkey: Some(HotkeySpec::parse("Ctrl+Shift+S").unwrap()),
            mode: Mode::Freeform,
            format: ImageFormat::Gif,
            folder: Some("E:/x".into()),
            ..CaptureSettings::default()
        };
        let back: CaptureSettings =
            serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
    }
}
