//! Where clips go and what they are called.

use std::path::{Path, PathBuf};

use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_INPROC_SERVER};
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::UI::Shell::{
    FOLDERID_Videos, FileOpenDialog, IFileOpenDialog, IShellItem, SHCreateItemFromParsingName,
    SHGetKnownFolderPath, ShellExecuteW, FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, KF_FLAG_DEFAULT,
    SIGDN_FILESYSPATH,
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

/// File name of a whole-session recording, e.g. `Dockview Recording 2026-11-05 09-04-03.mp4`.
pub fn recording_file_name(t: LocalTime) -> String {
    format!(
        "Dockview Recording {:04}-{:02}-{:02} {:02}-{:02}-{:02}.mp4",
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

/// The command-line argument that makes Explorer open a folder with `file`
/// highlighted.
pub fn reveal_args(file: &Path) -> String {
    format!("/select,\"{}\"", file.display())
}

/// Opens Explorer with `file` selected.
pub fn reveal_file(file: &Path) {
    let args = HSTRING::from(reveal_args(file));
    // SAFETY: plain shell call with valid, NUL-terminated strings.
    unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            w!("explorer.exe"),
            PCWSTR(args.as_ptr()),
            None,
            SW_SHOWNORMAL,
        );
    }
}

/// Whether files can be created in `folder` (it is created if missing).
pub fn is_writable_folder(folder: &Path) -> bool {
    if std::fs::create_dir_all(folder).is_err() {
        return false;
    }
    let probe = folder.join(format!(".dockview-write-test-{}", std::process::id()));
    let ok = std::fs::write(&probe, b"x").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

/// Asks the user for a folder with the standard Windows dialog, starting at
/// `start`. `None` if they cancel. Runs on the calling (UI) thread, which must
/// be a COM single-threaded apartment, as winit's is.
pub fn pick_folder(owner: Option<HWND>, start: &Path) -> Option<PathBuf> {
    // SAFETY: standard COM use; the returned string is CoTaskMem memory that is
    // copied and then freed exactly once.
    unsafe {
        let dialog: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let options = dialog.GetOptions().ok()?;
        dialog
            .SetOptions(options | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM)
            .ok()?;
        if let Ok(item) =
            SHCreateItemFromParsingName::<_, _, IShellItem>(&HSTRING::from(start.as_os_str()), None)
        {
            let _ = dialog.SetFolder(&item);
        }
        dialog.Show(owner).ok()?;
        let item = dialog.GetResult().ok()?;
        let name = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let path = name.to_string().ok().map(PathBuf::from);
        CoTaskMemFree(Some(name.0 as *const _));
        path
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
    fn explorer_is_asked_to_select_the_file() {
        let args = reveal_args(Path::new(r"C:\Videos\My Clip (2).mp4"));
        assert_eq!(args, "/select,\"C:\\Videos\\My Clip (2).mp4\"");
    }

    #[test]
    fn a_writable_folder_is_accepted_and_a_bad_one_is_not() {
        let dir = std::env::temp_dir().join(format!("dockview-writable-{}", std::process::id()));
        assert!(is_writable_folder(&dir), "created on demand");
        assert!(is_writable_folder(&dir), "and fine when it exists");
        assert!(
            std::fs::read_dir(&dir).unwrap().next().is_none(),
            "the probe file is removed"
        );
        let _ = std::fs::remove_dir(&dir);
        // A path whose parent is a file can never be a folder.
        let file = std::env::temp_dir().join(format!("dockview-notadir-{}", std::process::id()));
        std::fs::write(&file, b"x").unwrap();
        assert!(!is_writable_folder(&file.join("inside")));
        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn recordings_are_named_apart_from_clips() {
        let name = recording_file_name(time());
        assert_eq!(name, "Dockview Recording 2026-11-05 09-04-03.mp4");
        assert_ne!(name, clip_file_name(time()));
        assert!(!name.contains([':', '/', '\\', '*', '?', '"', '<', '>', '|']));
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
