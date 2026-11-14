//! What a screenshot is asked to take: the target, where it goes and how. The
//! command line (`--snap`) and, later, the capture overlay both build one of
//! these; taking it is the platform's business.

use std::path::PathBuf;

use crate::desktop::PxRect;
use crate::select::Mode;
use crate::sources::WindowInfo;

/// What to take a picture of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapTarget {
    /// Every monitor together (the whole virtual screen).
    Full,
    /// One monitor by position (0 is the primary).
    Monitor(usize),
    /// The first window whose title contains this text (any case).
    Window(String),
    /// A rectangle of the virtual screen, in pixels.
    Rect(PxRect),
    /// Let the user choose on the dimmed screen, starting in this mode.
    Select(Mode),
}

impl SnapTarget {
    /// `full`, `monitor:N`, `window:TEXT`, `rect:X,Y,W,H` or `select[:rect|window|full|free]`.
    pub fn parse(text: &str) -> Result<Self, String> {
        let text = text.trim();
        if text.eq_ignore_ascii_case("full") {
            return Ok(Self::Full);
        }
        if text.eq_ignore_ascii_case("select") {
            return Ok(Self::Select(Mode::Rectangle));
        }
        let (kind, value) = text.split_once(':').ok_or_else(|| {
            format!("'{text}': expected full, monitor:N, window:TEXT or rect:X,Y,W,H")
        })?;
        match kind.to_ascii_lowercase().as_str() {
            "monitor" => value
                .parse()
                .map(Self::Monitor)
                .map_err(|_| format!("monitor needs a number, got '{value}'")),
            "select" => match value.to_ascii_lowercase().as_str() {
                "rect" | "rectangle" => Ok(Self::Select(Mode::Rectangle)),
                "window" => Ok(Self::Select(Mode::Window)),
                "full" => Ok(Self::Select(Mode::Full)),
                "free" | "freeform" => Ok(Self::Select(Mode::Freeform)),
                other => Err(format!(
                    "unknown selection mode '{other}' (rect, window, full, free)"
                )),
            },
            "window" if !value.is_empty() => Ok(Self::Window(value.to_owned())),
            "window" => Err("window needs part of its title".into()),
            "rect" => {
                let parts: Vec<&str> = value.split(',').collect();
                let [x, y, w, h] = parts[..] else {
                    return Err("rect needs X,Y,W,H".into());
                };
                let num = |s: &str| -> Result<i32, String> {
                    s.trim()
                        .parse()
                        .map_err(|_| format!("'{s}' is not a number"))
                };
                let (w, h) = (num(w)?, num(h)?);
                if w <= 0 || h <= 0 {
                    return Err("rect needs a width and a height above zero".into());
                }
                Ok(Self::Rect(PxRect::new(
                    num(x)?,
                    num(y)?,
                    w as u32,
                    h as u32,
                )))
            }
            other => Err(format!(
                "unknown target '{other}' (full, monitor, window, rect)"
            )),
        }
    }
}

/// A screenshot to take: of what, where to save it, whether the cursor shows
/// and whether it also goes on the clipboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapJob {
    pub target: SnapTarget,
    pub out: Option<PathBuf>,
    pub cursor: bool,
    pub copy: bool,
    /// Seconds to wait before taking it (so a menu can be opened first).
    pub delay_secs: u32,
}

/// The delays the tool offers, in seconds (none, 3, 5, 10); anything up to a
/// minute is accepted on the command line.
pub const DELAYS: [u32; 4] = [0, 3, 5, 10];
/// The longest delay accepted.
pub const MAX_DELAY_SECS: u32 = 60;

/// The window to capture: the first whose title has `text`, never one of our
/// own (`own_pid`), so that asking for "Dockview" does not take the tool itself.
pub fn find_window<'a>(
    windows: &'a [WindowInfo],
    text: &str,
    own_pid: u32,
) -> Option<&'a WindowInfo> {
    let wanted = text.to_lowercase();
    windows
        .iter()
        .find(|w| w.process_id != own_pid && w.title.to_lowercase().contains(&wanted))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(title: &str, pid: u32) -> WindowInfo {
        WindowInfo {
            handle: 1,
            title: title.into(),
            process_id: pid,
        }
    }

    #[test]
    fn targets_are_read_from_their_short_forms() {
        assert_eq!(SnapTarget::parse("full"), Ok(SnapTarget::Full));
        assert_eq!(SnapTarget::parse(" FULL "), Ok(SnapTarget::Full));
        assert_eq!(SnapTarget::parse("monitor:1"), Ok(SnapTarget::Monitor(1)));
        assert_eq!(
            SnapTarget::parse("window:Mi juego"),
            Ok(SnapTarget::Window("Mi juego".into()))
        );
        assert_eq!(
            SnapTarget::parse("rect:-100,20,300,200"),
            Ok(SnapTarget::Rect(PxRect::new(-100, 20, 300, 200)))
        );
    }

    #[test]
    fn select_asks_the_user_in_the_mode_named_or_by_rectangle() {
        assert_eq!(
            SnapTarget::parse("select"),
            Ok(SnapTarget::Select(Mode::Rectangle))
        );
        assert_eq!(
            SnapTarget::parse("select:window"),
            Ok(SnapTarget::Select(Mode::Window))
        );
        assert_eq!(
            SnapTarget::parse("select:FULL"),
            Ok(SnapTarget::Select(Mode::Full))
        );
        assert_eq!(
            SnapTarget::parse("select:free"),
            Ok(SnapTarget::Select(Mode::Freeform))
        );
        assert!(SnapTarget::parse("select:circle").is_err());
    }

    #[test]
    fn a_window_title_may_contain_a_colon() {
        assert_eq!(
            SnapTarget::parse("window:C:\\Users - Terminal"),
            Ok(SnapTarget::Window("C:\\Users - Terminal".into()))
        );
    }

    #[test]
    fn nonsense_is_said_not_guessed() {
        for bad in [
            "",
            "monitor",
            "monitor:x",
            "window:",
            "rect:1,2,3",
            "rect:1,2,0,5",
            "rect:1,2,a,5",
            "thing:1",
            "rect:1,2,-3,4",
        ] {
            assert!(SnapTarget::parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn the_window_is_found_by_part_of_its_title_but_never_our_own() {
        let list = [
            window("Dockview", 10),
            window("Mi Juego - Chrome", 20),
            window("Mi juego 2", 30),
        ];
        assert_eq!(find_window(&list, "mi juego", 99).unwrap().process_id, 20);
        assert_eq!(find_window(&list, "dockview", 10), None);
        assert_eq!(find_window(&list, "dockview", 99).unwrap().process_id, 10);
        assert_eq!(find_window(&list, "nada", 99), None);
    }
}
