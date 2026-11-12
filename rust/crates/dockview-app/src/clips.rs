//! Where clips go and what they are called.

use std::path::{Path, PathBuf};

use windows::core::{w, HSTRING};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::UI::Shell::{
    FOLDERID_Videos, SHGetKnownFolderPath, ShellExecuteW, KF_FLAG_DEFAULT,
};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

/// A wall-clock moment, for naming files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalTime {
    pub year: u16,
    pub month: u16,
    pub day: u16,
    pub hour: u16,
    pub minute: u16,
    pub second: u16,
}

/// The current local time.
pub fn local_time_now() -> LocalTime {
    // SAFETY: GetLocalTime has no preconditions.
    let t = unsafe { GetLocalTime() };
    LocalTime {
        year: t.wYear,
        month: t.wMonth,
        day: t.wDay,
        hour: t.wHour,
        minute: t.wMinute,
        second: t.wSecond,
    }
}

/// `Dockview 2026-11-11 21-04-33.mp4`: sorts by date, and uses no character
/// that Windows forbids in file names.
pub fn clip_file_name(t: LocalTime) -> String {
    format!(
        "Dockview {:04}-{:02}-{:02} {:02}-{:02}-{:02}.mp4",
        t.year, t.month, t.day, t.hour, t.minute, t.second
    )
}

/// `dir/name`, or `dir/name (2)`, `(3)`… when that file already exists.
pub fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((stem, ext)) => (stem, format!(".{ext}")),
        None => (name, String::new()),
    };
    (2..)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|p| !p.exists())
        .unwrap_or(first)
}

/// The folder clips are saved to: `<Videos>\Dockview`.
pub fn default_folder() -> PathBuf {
    known_videos_folder()
        .or_else(|| std::env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join("Videos")))
        .unwrap_or_else(std::env::temp_dir)
        .join("Dockview")
}

fn known_videos_folder() -> Option<PathBuf> {
    // SAFETY: the returned string is CoTaskMem memory that is copied and then
    // freed exactly once.
    unsafe {
        let path = SHGetKnownFolderPath(&FOLDERID_Videos, KF_FLAG_DEFAULT, None).ok()?;
        let text = path.to_string().ok();
        CoTaskMemFree(Some(path.0 as *const _));
        text.map(PathBuf::from)
    }
}

/// Opens `folder` in Explorer, creating it first so there is something to open.
pub fn open_folder(folder: &Path) {
    let _ = std::fs::create_dir_all(folder);
    let target = HSTRING::from(folder.as_os_str());
    // SAFETY: plain shell call with valid, NUL-terminated strings.
    unsafe {
        ShellExecuteW(None, w!("open"), &target, None, None, SW_SHOWNORMAL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn time() -> LocalTime {
        LocalTime {
            year: 2026,
            month: 11,
            day: 5,
            hour: 9,
            minute: 4,
            second: 3,
        }
    }

    #[test]
    fn file_names_sort_by_time_and_avoid_forbidden_characters() {
        let name = clip_file_name(time());
        assert_eq!(name, "Dockview 2026-11-05 09-04-03.mp4");
        assert!(!name.contains([':', '/', '\\', '*', '?', '"', '<', '>', '|']));
        let later = clip_file_name(LocalTime {
            second: 4,
            ..time()
        });
        assert!(later > name);
    }

    #[test]
    fn an_existing_file_gets_a_numbered_sibling() {
        let dir = std::env::temp_dir().join(format!("dockview-clips-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let name = "clip.mp4";
        assert_eq!(unique_path(&dir, name), dir.join("clip.mp4"));
        std::fs::write(dir.join("clip.mp4"), b"x").unwrap();
        assert_eq!(unique_path(&dir, name), dir.join("clip (2).mp4"));
        std::fs::write(dir.join("clip (2).mp4"), b"x").unwrap();
        assert_eq!(unique_path(&dir, name), dir.join("clip (3).mp4"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_default_folder_is_dockview_inside_videos() {
        let folder = default_folder();
        assert_eq!(folder.file_name().unwrap(), "Dockview");
        assert!(folder.to_string_lossy().to_lowercase().contains("videos"));
    }

    #[test]
    fn the_local_time_is_plausible() {
        let t = local_time_now();
        assert!(t.year >= 2024 && (1..=12).contains(&t.month) && (1..=31).contains(&t.day));
        assert!(t.hour < 24 && t.minute < 60 && t.second < 61);
    }
}
