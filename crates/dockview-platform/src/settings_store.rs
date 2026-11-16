//! Settings kept in a file: `%APPDATA%\Dockview\settings.json`. This is the
//! real implementation of the domain's [`SettingsStore`] port.

use std::fs;
use std::path::{Path, PathBuf};

use dockview_domain::settings::{AppSettings, SettingsStore};

/// Location of the settings file: `%APPDATA%\Dockview\settings.json`.
pub fn settings_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|dir| PathBuf::from(dir).join("Dockview").join("settings.json"))
}

/// Where a file that could not be read is kept, next to the settings file, so
/// the next save does not destroy what the user had.
pub fn corrupt_copy_path(path: &Path) -> PathBuf {
    path.with_file_name("settings.corrupt.json")
}

/// Loads settings from `path`. A missing or unreadable file yields the
/// defaults; this never fails. A file that exists but is damaged is first
/// copied to `settings.corrupt.json`.
pub fn load_from(path: &Path) -> AppSettings {
    let Ok(bytes) = fs::read(path) else {
        return AppSettings::default();
    };
    let parsed = std::str::from_utf8(&bytes)
        .ok()
        .and_then(|text| AppSettings::try_from_json(text).ok());
    match parsed {
        Some(settings) => settings,
        None => {
            // An empty file holds nothing worth keeping (and must not replace
            // an earlier copy that does).
            if !bytes.iter().all(u8::is_ascii_whitespace) {
                let _ = fs::copy(path, corrupt_copy_path(path));
            }
            AppSettings::default()
        }
    }
}

/// Saves settings to `path`, creating parent directories as needed. The file
/// is written whole to a temporary one and then moved over the old one, so a
/// crash or a full disk in the middle never leaves half a file.
pub fn save_to(path: &Path, settings: &AppSettings) -> std::io::Result<()> {
    use std::io::Write;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = settings.to_json().map_err(std::io::Error::other)?;
    let temporary = path.with_extension("json.tmp");
    let written = fs::File::create(&temporary).and_then(|mut file| {
        file.write_all(json.as_bytes())?;
        file.sync_all()
    });
    if let Err(e) = written.and_then(|()| fs::rename(&temporary, path)) {
        let _ = fs::remove_file(&temporary);
        return Err(e);
    }
    Ok(())
}

/// The settings file, at the default location unless another is given.
#[derive(Debug, Clone, Default)]
pub struct FileSettingsStore {
    path: Option<PathBuf>,
}

impl FileSettingsStore {
    /// The default location (`%APPDATA%\Dockview\settings.json`).
    pub fn at_default_location() -> Self {
        Self {
            path: settings_path(),
        }
    }

    pub fn at(path: PathBuf) -> Self {
        Self { path: Some(path) }
    }
}

impl SettingsStore for FileSettingsStore {
    fn load(&self) -> AppSettings {
        self.path
            .as_deref()
            .map_or_else(AppSettings::default, load_from)
    }

    fn save(&self, settings: &AppSettings) -> Result<(), String> {
        let path = self
            .path
            .as_deref()
            .ok_or("there is no place to keep the settings")?;
        save_to(path, settings).map_err(|e| format!("{}: {e}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dockview_domain::settings::RenderPreset;

    fn temp_file(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dockview-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir.join("settings.json")
    }

    #[test]
    fn missing_file_returns_defaults() {
        let path = temp_file("missing");
        assert_eq!(load_from(&path), AppSettings::default());
    }

    #[test]
    fn a_file_with_a_utf8_byte_order_mark_still_loads() {
        let path = temp_file("bom");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "\u{feff}{ \"volume\": 0.25 }").unwrap();
        assert_eq!(load_from(&path).volume, 0.25);
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn corrupt_file_returns_defaults() {
        let path = temp_file("corrupt");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{invalid}").unwrap();
        assert_eq!(load_from(&path), AppSettings::default());
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_damaged_file_is_kept_as_a_copy_before_the_defaults_replace_it() {
        let path = temp_file("backup");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{ \"volume\": 0.5, oops").unwrap();
        assert_eq!(load_from(&path), AppSettings::default());
        assert_eq!(
            fs::read_to_string(corrupt_copy_path(&path)).unwrap(),
            "{ \"volume\": 0.5, oops"
        );
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_file_that_is_not_text_is_kept_too() {
        let path = temp_file("binary");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, [0xff, 0xfe, 0x00, 0x80]).unwrap();
        assert_eq!(load_from(&path), AppSettings::default());
        assert_eq!(
            fs::read(corrupt_copy_path(&path)).unwrap(),
            [0xff, 0xfe, 0x00, 0x80]
        );
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_missing_or_empty_file_leaves_no_copy() {
        let path = temp_file("nocopy");
        assert_eq!(load_from(&path), AppSettings::default());
        assert!(!corrupt_copy_path(&path).exists());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "  \n").unwrap();
        assert_eq!(load_from(&path), AppSettings::default());
        assert!(!corrupt_copy_path(&path).exists());
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn saving_leaves_only_the_settings_file_and_replaces_the_old_one() {
        let path = temp_file("atomic");
        let first = AppSettings {
            volume: 0.2,
            ..AppSettings::default()
        };
        let second = AppSettings {
            volume: 0.9,
            ..AppSettings::default()
        };
        save_to(&path, &first).unwrap();
        save_to(&path, &second).unwrap();
        assert_eq!(load_from(&path), second);
        let names: Vec<_> = fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["settings.json"], "no temporary file may stay");
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_temporary_file_left_by_a_crash_does_not_get_in_the_way() {
        let path = temp_file("stale");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path.with_extension("json.tmp"), "half a fi").unwrap();
        let settings = AppSettings {
            volume: 0.4,
            ..AppSettings::default()
        };
        save_to(&path, &settings).unwrap();
        assert_eq!(load_from(&path), settings);
        assert!(!path.with_extension("json.tmp").exists());
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn save_then_load_round_trips_and_creates_directories() {
        let path = temp_file("roundtrip");
        let settings = AppSettings {
            audio_offset_ms: 120,
            volume: 0.5,
            render_preset: RenderPreset::Performance,
            ..AppSettings::default()
        };
        save_to(&path, &settings).unwrap();
        assert_eq!(load_from(&path), settings);
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn the_file_store_saves_and_loads_through_the_port() {
        let path = temp_file("store");
        let store = FileSettingsStore::at(path.clone());
        assert_eq!(store.load(), AppSettings::default());
        let settings = AppSettings {
            volume: 0.25,
            ..AppSettings::default()
        };
        store.save(&settings).unwrap();
        assert_eq!(store.load(), settings);
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }
}
