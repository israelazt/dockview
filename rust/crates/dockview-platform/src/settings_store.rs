//! Settings kept in a file: `%APPDATA%\Dockview\settings.json`. This is the
//! real implementation of the domain's [`SettingsStore`] port.

use std::fs;
use std::path::{Path, PathBuf};

use dockview_domain::settings::{AppSettings, SettingsStore};

/// Location of the settings file: `%APPDATA%\Dockview\settings.json`.
pub fn settings_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|dir| PathBuf::from(dir).join("Dockview").join("settings.json"))
}

/// Loads settings from `path`. A missing, unreadable or corrupt file yields
/// the defaults; this never fails.
pub fn load_from(path: &Path) -> AppSettings {
    fs::read_to_string(path)
        .map(|json| AppSettings::from_json(&json))
        .unwrap_or_default()
}

/// Saves settings to `path`, creating parent directories as needed.
pub fn save_to(path: &Path, settings: &AppSettings) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = settings.to_json().map_err(std::io::Error::other)?;
    fs::write(path, json)
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

    fn save(&self, settings: &AppSettings) {
        if let Some(path) = &self.path {
            let _ = save_to(path, settings);
        }
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
        store.save(&settings);
        assert_eq!(store.load(), settings);
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }
}
