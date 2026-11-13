//! User preferences: the model, its defaults and the rules that keep it in
//! range. Reading and writing the file is the platform's job (see the
//! [`SettingsStore`] port); the file lives at `%APPDATA%\Dockview\settings.json`.
//!
//! The on-disk format is compatible with the original C# app (camelCase keys,
//! `renderPreset` stored as a number) so both versions can share one file
//! during the migration.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::scene::Scene;

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

/// Quality of the video the replay buffer and recordings are encoded at. One
/// encoder serves both, so it is one choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingQuality {
    /// Smaller files, still good for sharing (about 7 GB per hour).
    Normal,
    /// Best picture, big files (about 18 GB per hour).
    High,
}

/// Video bitrate of [`RecordingQuality::Normal`], in megabits per second.
pub const NORMAL_REPLAY_MBPS: u32 = 16;

impl RecordingQuality {
    pub const ALL: [Self; 2] = [Self::Normal, Self::High];

    pub fn mbps(self) -> u32 {
        match self {
            Self::Normal => NORMAL_REPLAY_MBPS,
            Self::High => DEFAULT_REPLAY_MBPS,
        }
    }

    /// The quality closest to a stored bitrate (a hand-edited value snaps to
    /// the nearer one).
    pub fn from_mbps(mbps: u32) -> Self {
        if mbps.abs_diff(NORMAL_REPLAY_MBPS) < mbps.abs_diff(DEFAULT_REPLAY_MBPS) {
            Self::Normal
        } else {
            Self::High
        }
    }
}

/// Bytes a recording takes per hour at `mbps`.
pub fn recording_bytes_per_hour(mbps: u32) -> u64 {
    u64::from(mbps) * 125_000 * 3600
}

/// Lengths offered for the instant-replay buffer, in seconds.
pub const REPLAY_STEPS: [u32; 15] = [
    5, 10, 15, 20, 30, 45, 60, 90, 120, 180, 240, 300, 420, 600, 900,
];
/// Default replay length.
pub const DEFAULT_REPLAY_SECONDS: u32 = 30;
/// Default video bitrate for the replay buffer, in megabits per second.
pub const DEFAULT_REPLAY_MBPS: u32 = 40;

/// Extra seconds the recorder keeps beyond the clip length, so a clip of the
/// chosen length always fits (it starts at the keyframe before the cut).
pub const REPLAY_MARGIN_SECONDS: u32 = 2;
/// Share of the installed RAM the replay buffer may use.
pub const REPLAY_RAM_SHARE_PERCENT: u64 = 20;

/// Memory the buffer needs for `seconds` of footage at `bitrate_mbps`, in bytes.
pub fn replay_memory_bytes(seconds: u32, bitrate_mbps: u32) -> u64 {
    u64::from(seconds + REPLAY_MARGIN_SECONDS) * u64::from(bitrate_mbps) * 125_000
}

/// The longest listed length whose buffer fits in [`REPLAY_RAM_SHARE_PERCENT`]
/// of `total_ram_bytes`. Never below the shortest length. An unknown amount of
/// RAM (`0`) imposes no limit.
pub fn max_replay_seconds(bitrate_mbps: u32, total_ram_bytes: u64) -> u32 {
    if total_ram_bytes == 0 {
        return REPLAY_STEPS[REPLAY_STEPS.len() - 1];
    }
    let budget = total_ram_bytes / 100 * REPLAY_RAM_SHARE_PERCENT;
    REPLAY_STEPS
        .iter()
        .copied()
        .rfind(|&s| replay_memory_bytes(s, bitrate_mbps) <= budget)
        .unwrap_or(REPLAY_STEPS[0])
}

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

/// The volume after moving it by `delta` (0.0 to 1.0), rounded to whole
/// percents so repeated 5% steps stay exact instead of drifting.
pub fn step_volume(current: f32, delta: f32) -> f32 {
    let moved = (current + delta).clamp(0.0, 1.0);
    (moved * 100.0).round() / 100.0
}

/// The audio sync offset after moving it by `delta_ms`, within the allowed range.
pub fn step_audio_offset(current_ms: i32, delta_ms: i32) -> i32 {
    (current_ms + delta_ms).clamp(-MAX_AUDIO_OFFSET_MS, MAX_AUDIO_OFFSET_MS)
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
    /// WASAPI id of the chosen microphone; `None` is the Windows default. The
    /// microphone itself always starts off: it is turned on by hand each session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_mic_device_id: Option<String>,
    /// The monitor (by position in the list, 0 is the primary) that was being
    /// captured when the program closed; `None` means the capture card.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_monitor: Option<u32>,
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
    /// The scenes the user made.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub scenes: Vec<Scene>,
    /// The scene in use (a position in `scenes`); `None` is the plain source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_scene: Option<usize>,
    /// Names the user gave capture devices, by USB `vid:pid` (for example
    /// `"345f:2131"`). They are only shown; the device keeps its real name.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub device_aliases: BTreeMap<String, String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            last_video_device_symbolic_link: None,
            last_audio_device_id: None,
            last_mic_device_id: None,
            last_monitor: None,
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
            scenes: Vec::new(),
            active_scene: None,
            device_aliases: BTreeMap::new(),
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
        self.scenes = self
            .scenes
            .into_iter()
            .map(Scene::sanitized)
            .filter(|s| !s.layers.is_empty())
            .collect();
        self.device_aliases = crate::device_names::sanitize_aliases(self.device_aliases);
        // The scene in use must exist.
        self.active_scene = self.active_scene.filter(|&i| i < self.scenes.len());
        self
    }
}

impl AppSettings {
    /// Reads settings from JSON text. A damaged or empty text yields the
    /// defaults; this never fails, and out-of-range values are clamped.
    pub fn from_json(json: &str) -> Self {
        // Editors such as Notepad (and PowerShell 5.1) prepend a UTF-8 byte
        // order mark, which JSON parsers reject.
        serde_json::from_str::<Self>(json.trim_start_matches('\u{feff}'))
            .unwrap_or_default()
            .sanitized()
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

/// Where settings are kept between runs. The real one writes a file
/// (`dockview-platform`); tests use [`MemoryStore`].
pub trait SettingsStore {
    /// The saved settings, or the defaults when there are none. Never fails.
    fn load(&self) -> AppSettings;
    /// Saves the settings. A failure is not fatal: they reset on the next run.
    fn save(&self, settings: &AppSettings);
}

/// A store that keeps the settings in memory, for tests and previews.
#[derive(Debug, Default)]
pub struct MemoryStore {
    saved: std::sync::Mutex<Option<AppSettings>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// How the settings were last saved, if they were.
    pub fn last_saved(&self) -> Option<AppSettings> {
        self.saved.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl SettingsStore for MemoryStore {
    fn load(&self) -> AppSettings {
        self.last_saved().unwrap_or_default()
    }

    fn save(&self, settings: &AppSettings) {
        *self.saved.lock().unwrap_or_else(|e| e.into_inner()) = Some(settings.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::SourceRef;

    #[test]
    fn json_with_a_byte_order_mark_or_garbage_is_read_safely() {
        assert_eq!(
            AppSettings::from_json("\u{feff}{ \"volume\": 0.25 }").volume,
            0.25
        );
        assert_eq!(AppSettings::from_json("{invalid}"), AppSettings::default());
        assert_eq!(AppSettings::from_json(""), AppSettings::default());
    }

    #[test]
    fn the_memory_store_keeps_what_was_saved() {
        let store = MemoryStore::new();
        assert_eq!(store.load(), AppSettings::default());
        assert!(store.last_saved().is_none());
        let settings = AppSettings {
            volume: 0.5,
            ..AppSettings::default()
        };
        store.save(&settings);
        assert_eq!(store.load(), settings);
        assert_eq!(
            AppSettings::from_json(&settings.to_json().unwrap()),
            settings
        );
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
        assert!(s.last_mic_device_id.is_none());
        assert!(s.last_monitor.is_none());
    }

    #[test]
    fn round_trip_preserves_all_fields() {
        let original = AppSettings {
            last_video_device_symbolic_link: Some(r"\\?\usb#vid_1234".into()),
            last_audio_device_id: Some("{0.0.1.00000000}.{abc}".into()),
            last_mic_device_id: Some("{0.0.1.00000000}.{mic}".into()),
            last_monitor: Some(1),
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
            scenes: vec![Scene::with_base(
                "Juego",
                SourceRef::Capture {
                    symbolic_link: "usb#a".into(),
                },
            )],
            active_scene: Some(0),
            device_aliases: BTreeMap::from([("345f:2131".to_owned(), "Pacoxi".to_owned())]),
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
        assert!(
            defaults.replay_enabled,
            "the buffer is on unless the user turns it off"
        );
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
    fn quality_maps_to_bitrates_and_back() {
        assert_eq!(RecordingQuality::Normal.mbps(), 16);
        assert_eq!(RecordingQuality::High.mbps(), 40);
        for quality in RecordingQuality::ALL {
            assert_eq!(RecordingQuality::from_mbps(quality.mbps()), quality);
        }
        // Hand-edited values snap to the nearer quality.
        assert_eq!(RecordingQuality::from_mbps(5), RecordingQuality::Normal);
        assert_eq!(RecordingQuality::from_mbps(25), RecordingQuality::Normal);
        assert_eq!(RecordingQuality::from_mbps(29), RecordingQuality::High);
        assert_eq!(RecordingQuality::from_mbps(100), RecordingQuality::High);
        // The default stays what it was: high.
        assert_eq!(
            RecordingQuality::from_mbps(AppSettings::default().replay_bitrate_mbps),
            RecordingQuality::High
        );
    }

    #[test]
    fn an_hour_of_recording_is_sized_from_the_bitrate() {
        assert_eq!(recording_bytes_per_hour(16), 7_200_000_000);
        assert_eq!(recording_bytes_per_hour(40), 18_000_000_000);
    }

    #[test]
    fn replay_memory_estimate_and_ram_limit() {
        // 30 s + 2 s margin at 40 Mbps = 160 MB.
        assert_eq!(replay_memory_bytes(30, 40), 160_000_000);
        // 8 GB of RAM: budget 1.6 GB -> 300 s needs 1.51 GB, 420 s needs 2.1 GB.
        let eight_gb = 8 * 1024 * 1024 * 1024;
        assert_eq!(max_replay_seconds(40, eight_gb), 300);
        // 16 GB: 600 s needs 3.0 GB, 900 s needs 4.5 GB (budget 3.4 GB).
        assert_eq!(max_replay_seconds(40, 2 * eight_gb), 600);
        // 1 GB of RAM allows 30 s; the shortest length is always allowed; unknown RAM is unlimited.
        assert_eq!(max_replay_seconds(40, 1024 * 1024 * 1024), 30);
        assert_eq!(max_replay_seconds(100, 1), REPLAY_STEPS[0]);
        assert_eq!(max_replay_seconds(40, 0), 900);
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
    fn scenes_are_saved_and_an_old_file_without_them_still_loads() {
        use crate::scene::{Scene, SourceRef};
        let old = r#"{"volume":0.5}"#;
        let loaded: AppSettings = serde_json::from_str(old).unwrap();
        assert!(loaded.scenes.is_empty() && loaded.active_scene.is_none());
        // Nothing is written for a user who never made a scene.
        assert!(!serde_json::to_string(&loaded).unwrap().contains("scenes"));

        let mut with = AppSettings::default();
        with.scenes.push(Scene::with_base(
            "A",
            SourceRef::Window {
                title: "Juego".into(),
            },
        ));
        with.active_scene = Some(0);
        let json = serde_json::to_string(&with).unwrap();
        assert!(json.contains("activeScene"));
        assert_eq!(serde_json::from_str::<AppSettings>(&json).unwrap(), with);
    }

    #[test]
    fn sanitizing_drops_empty_scenes_and_a_dangling_active_one() {
        use crate::scene::{Scene, SourceRef};
        let mut s = AppSettings::default();
        s.scenes.push(Scene {
            name: "vacía".into(),
            layers: Vec::new(),
        });
        s.scenes.push(Scene::with_base(
            "buena",
            SourceRef::Monitor {
                device_name: "D".into(),
            },
        ));
        s.active_scene = Some(5);
        let s = s.sanitized();
        assert_eq!(s.scenes.len(), 1);
        assert_eq!(s.scenes[0].name, "buena");
        assert_eq!(s.active_scene, None);
    }

    #[test]
    fn volume_steps_stay_exact_and_inside_the_range() {
        let mut volume = 0.0;
        for _ in 0..7 {
            volume = step_volume(volume, 0.05);
        }
        assert_eq!(volume, 0.35, "no float drift after seven steps");
        assert_eq!(step_volume(0.98, 0.05), 1.0);
        assert_eq!(step_volume(0.02, -0.05), 0.0);
    }

    #[test]
    fn the_sync_offset_stops_at_both_ends() {
        assert_eq!(step_audio_offset(0, 10), 10);
        assert_eq!(step_audio_offset(-20, 10), -10);
        assert_eq!(
            step_audio_offset(MAX_AUDIO_OFFSET_MS - 5, 10),
            MAX_AUDIO_OFFSET_MS
        );
        assert_eq!(
            step_audio_offset(-MAX_AUDIO_OFFSET_MS + 5, -10),
            -MAX_AUDIO_OFFSET_MS
        );
    }
}
