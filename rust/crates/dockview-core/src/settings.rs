//! User preferences persisted to `%APPDATA%\Dockview\settings.json`.
//!
//! The on-disk format is compatible with the original C# app (camelCase keys,
//! `renderPreset` stored as a number) so both versions can share one file
//! during the migration.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Maximum audio sync offset in either direction, in milliseconds.
pub const MAX_AUDIO_OFFSET_MS: i32 = 300;

/// Render quality preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RenderPreset {
    /// Nearest-neighbour scaling, 30 fps render cap.
    Performance,
    /// Bilinear GPU scaling, 60 fps.
    #[default]
    Balanced,
    /// Highest-quality scaling, 60 fps.
    Quality,
}

impl RenderPreset {
    fn as_index(self) -> u8 {
        match self {
            Self::Performance => 0,
            Self::Balanced => 1,
            Self::Quality => 2,
        }
    }
}

impl Serialize for RenderPreset {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(self.as_index())
    }
}

impl<'de> Deserialize<'de> for RenderPreset {
    /// Accepts the numeric form written by the C# app and the variant name.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Repr {
            Index(u8),
            Name(String),
        }

        match Repr::deserialize(deserializer)? {
            Repr::Index(0) => Ok(Self::Performance),
            Repr::Index(1) => Ok(Self::Balanced),
            Repr::Index(2) => Ok(Self::Quality),
            Repr::Name(n) if n.eq_ignore_ascii_case("performance") => Ok(Self::Performance),
            Repr::Name(n) if n.eq_ignore_ascii_case("balanced") => Ok(Self::Balanced),
            Repr::Name(n) if n.eq_ignore_ascii_case("quality") => Ok(Self::Quality),
            _ => Err(serde::de::Error::custom("unknown render preset")),
        }
    }
}

/// Interface theme preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    /// Follow the Windows app theme.
    #[default]
    Auto,
    Dark,
    Light,
}

/// Interface language preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LangMode {
    /// Follow the Windows display language.
    #[default]
    Auto,
    En,
    Es,
}

/// How the video mode of the capture device is chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VideoModePref {
    /// Best quality first (uncompressed); switches to a compressed mode by
    /// itself if the USB connection cannot keep up.
    #[default]
    Auto,
    /// Always use a compressed (MJPEG) mode when the device offers one.
    Compressed,
}

/// Lengths offered for the instant-replay buffer, in seconds.
pub const REPLAY_STEPS: [u32; 15] = [
    5, 10, 15, 20, 30, 45, 60, 90, 120, 180, 240, 300, 420, 600, 900,
];
/// Default replay length.
pub const DEFAULT_REPLAY_SECONDS: u32 = 30;
/// Default video bitrate for the replay buffer, in megabits per second.
pub const DEFAULT_REPLAY_MBPS: u32 = 40;

/// Moves `current` one step along [`REPLAY_STEPS`] (`delta` of +1 or -1),
/// snapping first to the nearest listed length. Stops at either end.
pub fn step_replay_seconds(current: u32, delta: i32) -> u32 {
    let nearest = REPLAY_STEPS
        .iter()
        .enumerate()
        .min_by_key(|(_, s)| s.abs_diff(current))
        .map_or(0, |(i, _)| i);
    let index = (nearest as i32 + delta).clamp(0, REPLAY_STEPS.len() as i32 - 1) as usize;
    REPLAY_STEPS[index]
}

/// Persisted application settings. Every field falls back to its default when
/// missing from the file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    /// Symbolic link of the last-used video capture device.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_video_device_symbolic_link: Option<String>,
    /// WASAPI id of the last-used audio capture device.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_audio_device_id: Option<String>,
    /// Audio sync offset in milliseconds (-300..=300).
    pub audio_offset_ms: i32,
    /// Master volume (0.0..=1.0).
    pub volume: f32,
    /// Whether the window was last in fullscreen mode.
    pub was_fullscreen: bool,
    /// Render quality preset.
    pub render_preset: RenderPreset,
    /// Interface theme preference.
    pub theme: ThemeMode,
    /// Interface language preference.
    pub language: LangMode,
    /// How the capture mode is chosen.
    pub video_mode: VideoModePref,
    /// Whether the instant-replay buffer runs. On by default; the user turns it
    /// off by hand.
    pub replay_enabled: bool,
    /// Seconds of footage the instant replay keeps and saves.
    pub replay_seconds: u32,
    /// Video bitrate of the instant-replay buffer, in Mbps.
    pub replay_bitrate_mbps: u32,
    /// Where clips are saved; `None` means the Videos\\Dockview folder.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replay_folder: Option<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            last_video_device_symbolic_link: None,
            last_audio_device_id: None,
            audio_offset_ms: 0,
            volume: 1.0,
            was_fullscreen: false,
            render_preset: RenderPreset::default(),
            theme: ThemeMode::default(),
            language: LangMode::default(),
            video_mode: VideoModePref::default(),
            replay_enabled: true,
            replay_seconds: DEFAULT_REPLAY_SECONDS,
            replay_bitrate_mbps: DEFAULT_REPLAY_MBPS,
            replay_folder: None,
        }
    }
}

impl AppSettings {
    /// Returns a copy with out-of-range values clamped to their valid range.
    #[must_use]
    pub fn sanitized(mut self) -> Self {
        self.audio_offset_ms = self
            .audio_offset_ms
            .clamp(-MAX_AUDIO_OFFSET_MS, MAX_AUDIO_OFFSET_MS);
        self.replay_seconds = self
            .replay_seconds
            .clamp(REPLAY_STEPS[0], REPLAY_STEPS[REPLAY_STEPS.len() - 1]);
        self.replay_bitrate_mbps = self.replay_bitrate_mbps.clamp(5, 100);
        self.volume = if self.volume.is_nan() {
            1.0
        } else {
            self.volume.clamp(0.0, 1.0)
        };
        self
    }
}

/// Location of the settings file: `%APPDATA%\Dockview\settings.json`.
pub fn settings_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|dir| PathBuf::from(dir).join("Dockview").join("settings.json"))
}

/// Loads settings from `path`. A missing, unreadable or corrupt file yields
/// the defaults; this never fails.
pub fn load_from(path: &Path) -> AppSettings {
    fs::read_to_string(path)
        .ok()
        // Editors such as Notepad (and PowerShell 5.1) prepend a UTF-8 byte
        // order mark, which JSON parsers reject.
        .and_then(|json| {
            serde_json::from_str::<AppSettings>(json.trim_start_matches('\u{feff}')).ok()
        })
        .unwrap_or_default()
        .sanitized()
}

/// Saves settings to `path`, creating parent directories as needed.
pub fn save_to(path: &Path, settings: &AppSettings) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(settings).map_err(std::io::Error::other)?;
    fs::write(path, json)
}

/// Loads settings from the default location.
pub fn load() -> AppSettings {
    settings_path().map_or_else(AppSettings::default, |p| load_from(&p))
}

/// Saves settings to the default location. Failure is non-fatal: settings
/// simply reset on next launch.
pub fn save(settings: &AppSettings) {
    if let Some(path) = settings_path() {
        let _ = save_to(&path, settings);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dockview-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir.join("settings.json")
    }

    #[test]
    fn default_values_are_correct() {
        let s = AppSettings::default();
        assert_eq!(s.audio_offset_ms, 0);
        assert_eq!(s.volume, 1.0);
        assert!(!s.was_fullscreen);
        assert_eq!(s.render_preset, RenderPreset::Balanced);
        assert!(s.last_video_device_symbolic_link.is_none());
        assert!(s.last_audio_device_id.is_none());
    }

    #[test]
    fn round_trip_preserves_all_fields() {
        let original = AppSettings {
            last_video_device_symbolic_link: Some(r"\\?\usb#vid_1234".into()),
            last_audio_device_id: Some("{0.0.1.00000000}.{abc}".into()),
            audio_offset_ms: -150,
            volume: 0.75,
            was_fullscreen: true,
            render_preset: RenderPreset::Quality,
            theme: ThemeMode::Light,
            language: LangMode::Es,
            video_mode: VideoModePref::Compressed,
            replay_enabled: false,
            replay_seconds: 120,
            replay_bitrate_mbps: 25,
            replay_folder: Some(r"D:\\Clips".into()),
        };
        let json = serde_json::to_string_pretty(&original).unwrap();
        let restored: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(original, restored);
    }

    #[test]
    fn empty_json_object_yields_defaults() {
        let s: AppSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(s, AppSettings::default());
    }

    #[test]
    fn none_fields_are_omitted_and_keys_are_camel_case() {
        let json = serde_json::to_string(&AppSettings::default()).unwrap();
        assert!(!json.contains("lastVideoDeviceSymbolicLink"));
        assert!(json.contains("audioOffsetMs"));
        assert!(json.contains("wasFullscreen"));
    }

    #[test]
    fn reads_csharp_numeric_preset_and_writes_it_back() {
        let s: AppSettings = serde_json::from_str(r#"{"renderPreset": 2}"#).unwrap();
        assert_eq!(s.render_preset, RenderPreset::Quality);
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains(r#""renderPreset":2"#));
    }

    #[test]
    fn reads_preset_by_name_case_insensitive() {
        let s: AppSettings = serde_json::from_str(r#"{"renderPreset": "Performance"}"#).unwrap();
        assert_eq!(s.render_preset, RenderPreset::Performance);
    }

    #[test]
    fn theme_and_language_are_stored_as_lowercase_words() {
        let s = AppSettings {
            theme: ThemeMode::Dark,
            language: LangMode::En,
            ..AppSettings::default()
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains(r#""theme":"dark""#));
        assert!(json.contains(r#""language":"en""#));
    }

    #[test]
    fn video_mode_is_stored_as_a_lowercase_word_and_defaults_to_auto() {
        let s = AppSettings {
            video_mode: VideoModePref::Compressed,
            ..AppSettings::default()
        };
        assert!(serde_json::to_string(&s)
            .unwrap()
            .contains(r#""videoMode":"compressed""#));
        let old: AppSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(old.video_mode, VideoModePref::Auto);
    }

    #[test]
    fn replay_settings_default_round_trip_and_stay_in_range() {
        let defaults = AppSettings::default();
        assert!(defaults.replay_enabled, "the buffer is on unless the user turns it off");
        assert_eq!(defaults.replay_seconds, 30);
        assert_eq!(defaults.replay_bitrate_mbps, 40);
        assert!(defaults.replay_folder.is_none());

        let wild = AppSettings {
            replay_seconds: 99_999,
            replay_bitrate_mbps: 0,
            ..AppSettings::default()
        }
        .sanitized();
        assert_eq!(wild.replay_seconds, 900);
        assert_eq!(wild.replay_bitrate_mbps, 5);
        let tiny = AppSettings {
            replay_seconds: 1,
            ..AppSettings::default()
        }
        .sanitized();
        assert_eq!(tiny.replay_seconds, 5);
    }

    #[test]
    fn replay_length_steps_walk_the_list_and_stop_at_the_ends() {
        assert_eq!(step_replay_seconds(30, 1), 45);
        assert_eq!(step_replay_seconds(30, -1), 20);
        assert_eq!(step_replay_seconds(5, -1), 5, "stops at the shortest");
        assert_eq!(step_replay_seconds(900, 1), 900, "stops at the longest");
        // An unlisted value snaps to its nearest step before moving.
        assert_eq!(step_replay_seconds(33, 1), 45);
        assert_eq!(step_replay_seconds(200, -1), 120);
    }

    #[test]
    fn the_listed_lengths_are_sorted_and_include_the_default() {
        assert!(REPLAY_STEPS.windows(2).all(|w| w[0] < w[1]));
        assert!(REPLAY_STEPS.contains(&DEFAULT_REPLAY_SECONDS));
    }

    #[test]
    fn files_from_the_csharp_app_load_with_auto_theme_and_language() {
        let s: AppSettings = serde_json::from_str(r#"{"volume": 0.5}"#).unwrap();
        assert_eq!((s.theme, s.language), (ThemeMode::Auto, LangMode::Auto));
    }

    #[test]
    fn unknown_preset_is_rejected() {
        assert!(serde_json::from_str::<AppSettings>(r#"{"renderPreset": 9}"#).is_err());
    }

    #[test]
    fn sanitized_clamps_out_of_range_values() {
        let s = AppSettings {
            audio_offset_ms: 9999,
            volume: 3.0,
            ..AppSettings::default()
        }
        .sanitized();
        assert_eq!(s.audio_offset_ms, MAX_AUDIO_OFFSET_MS);
        assert_eq!(s.volume, 1.0);

        let s = AppSettings {
            audio_offset_ms: -9999,
            volume: f32::NAN,
            ..AppSettings::default()
        }
        .sanitized();
        assert_eq!(s.audio_offset_ms, -MAX_AUDIO_OFFSET_MS);
        assert_eq!(s.volume, 1.0);
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
}
