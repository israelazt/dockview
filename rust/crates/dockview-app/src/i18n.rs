//! UI strings in American English and Latin American Spanish.

/// Languages the interface is available in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    En,
    Es,
}

pub use dockview_core::settings::LangMode;

/// Resolving the stored preference against the detected system language.
pub trait LangModeExt {
    fn resolve(self, system: Lang) -> Lang;
}

impl LangModeExt for LangMode {
    /// Resolves the preference given the detected system language.
    fn resolve(self, system: Lang) -> Lang {
        match self {
            Self::Auto => system,
            Self::En => Lang::En,
            Self::Es => Lang::Es,
        }
    }
}

impl Lang {
    /// Maps a Windows LANGID to a supported language. Any Spanish variant
    /// maps to Spanish; everything else falls back to English.
    pub fn from_langid(langid: u16) -> Self {
        const LANG_SPANISH: u16 = 0x0A;
        if langid & 0x03FF == LANG_SPANISH {
            Self::Es
        } else {
            Self::En
        }
    }

    /// The language of the Windows display (UI) language.
    #[cfg(windows)]
    pub fn detect_system() -> Self {
        use windows::Win32::Globalization::GetUserDefaultUILanguage;
        // SAFETY: simple query with no arguments.
        Self::from_langid(unsafe { GetUserDefaultUILanguage() })
    }
}

/// Every translatable string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Live,
    Connecting,
    Disconnected,
    NoDevice,
    Volume,
    Sync,
    Settings,
    Fullscreen,
    Video,
    Audio,
    ImageQuality,
    PresetPerformance,
    PresetBalanced,
    PresetHigh,
    Theme,
    Language,
    Auto,
    Dark,
    Light,
    English,
    Spanish,
    Statistics,
    Performance,
    Display,
    Diagnosis,
    DeviceMode,
    Received,
    Shown,
    Dropped,
    AppLatency,
    Cpu,
    GpuPerFrame,
    Memory,
    AudioBuffer,
    Monitor,
    WindowSize,
    Bandwidth,
    Measuring,
    FindHealthy,
    FindNoVideo,
    FindVideoLost,
    FindNeedsUsb3,
    FindCaptureLow,
    FindDropping,
    FindHighCpu,
    FindSlowGpu,
    FindAudioLost,
    VideoMode,
    ModeCompressed,
    FindSwitched,
    InstantReplay,
    ReplayOff,
    ReplayRecording,
    ReplayStarting,
    ReplayUnavailable,
    ReplayKeep,
    ReplayHint,
    ReplayMemory,
    ReplayMemoryLimit,
    Microphone,
    MicDefault,
    MicHint,
    MicUnavailable,
    ToastMicOn,
    ToastMicOff,
    ToastRecStarted,
    ToastRecSaved,
    ToastRecFailed,
    ToastStopRecordingFirst,
    ToastRecDiskFull,
    ToastRecNewPart,
    ToastRecNoSignal,
    ToastLowDisk,
    ToastRecNoSpace,
    RecordingQuality,
    QualityNormal,
    QualityHigh,
    QualityHint,
    ChangeFolder,
    ToastFolderNotWritable,
    ToastQualityChanged,
    SourceCapture,
    SourceScreen,
    SourceApp,
    SourceScreenWord,
    SourcePrimary,
    ToastSourceLost,
    ToastSourceFailed,
    SaveClip,
    OpenFolder,
    ToastClipSaved,
    ToastClipFailed,
    ToastTurnOnFirst,
    ToastReplayOn,
    ToastReplayOff,
    ToastSaving,
}

/// A length of time as people say it: `45 s`, `2 min`, `1 min 30 s`.
pub fn duration_label(seconds: u32) -> String {
    match (seconds / 60, seconds % 60) {
        (0, s) => format!("{s} s"),
        (m, 0) => format!("{m} min"),
        (m, s) => format!("{m} min {s} s"),
    }
}

/// A memory size for people: `640 MB`, `1.5 GB`.
pub fn size_label(bytes: u64) -> String {
    let mb = bytes as f64 / 1e6;
    if mb < 1000.0 {
        format!("{:.0} MB", (mb / 10.0).round() * 10.0)
    } else {
        format!("{:.1} GB", mb / 1000.0)
    }
}

/// All keys, for completeness tests.
pub const ALL_KEYS: [Key; 96] = [
    Key::Live,
    Key::Connecting,
    Key::Disconnected,
    Key::NoDevice,
    Key::Volume,
    Key::Sync,
    Key::Settings,
    Key::Fullscreen,
    Key::Video,
    Key::Audio,
    Key::ImageQuality,
    Key::PresetPerformance,
    Key::PresetBalanced,
    Key::PresetHigh,
    Key::Theme,
    Key::Language,
    Key::Auto,
    Key::Dark,
    Key::Light,
    Key::English,
    Key::Spanish,
    Key::Statistics,
    Key::Performance,
    Key::Display,
    Key::Diagnosis,
    Key::DeviceMode,
    Key::Received,
    Key::Shown,
    Key::Dropped,
    Key::AppLatency,
    Key::Cpu,
    Key::GpuPerFrame,
    Key::Memory,
    Key::AudioBuffer,
    Key::Monitor,
    Key::WindowSize,
    Key::Bandwidth,
    Key::Measuring,
    Key::FindHealthy,
    Key::FindNoVideo,
    Key::FindVideoLost,
    Key::FindNeedsUsb3,
    Key::FindCaptureLow,
    Key::FindDropping,
    Key::FindHighCpu,
    Key::FindSlowGpu,
    Key::FindAudioLost,
    Key::VideoMode,
    Key::ModeCompressed,
    Key::FindSwitched,
    Key::InstantReplay,
    Key::ReplayOff,
    Key::ReplayRecording,
    Key::ReplayStarting,
    Key::ReplayUnavailable,
    Key::ReplayKeep,
    Key::ReplayHint,
    Key::ReplayMemory,
    Key::ReplayMemoryLimit,
    Key::Microphone,
    Key::MicDefault,
    Key::MicHint,
    Key::MicUnavailable,
    Key::ToastMicOn,
    Key::ToastMicOff,
    Key::ToastRecStarted,
    Key::ToastRecSaved,
    Key::ToastRecFailed,
    Key::ToastStopRecordingFirst,
    Key::ToastRecDiskFull,
    Key::ToastRecNewPart,
    Key::ToastRecNoSignal,
    Key::ToastLowDisk,
    Key::ToastRecNoSpace,
    Key::RecordingQuality,
    Key::QualityNormal,
    Key::QualityHigh,
    Key::QualityHint,
    Key::ChangeFolder,
    Key::ToastFolderNotWritable,
    Key::ToastQualityChanged,
    Key::SourceCapture,
    Key::SourceScreen,
    Key::SourceApp,
    Key::SourceScreenWord,
    Key::SourcePrimary,
    Key::ToastSourceLost,
    Key::ToastSourceFailed,
    Key::SaveClip,
    Key::OpenFolder,
    Key::ToastClipSaved,
    Key::ToastClipFailed,
    Key::ToastTurnOnFirst,
    Key::ToastReplayOn,
    Key::ToastReplayOff,
    Key::ToastSaving,
];

/// Looks up `key` in `lang`. The matches have no wildcard arm, so adding a
/// [`Key`] without translating it in both languages does not compile.
pub fn tr(lang: Lang, key: Key) -> &'static str {
    match lang {
        Lang::En => en(key),
        Lang::Es => es(key),
    }
}

fn en(key: Key) -> &'static str {
    match key {
        Key::Live => "Live",
        Key::Connecting => "Connecting…",
        Key::Disconnected => "Disconnected",
        Key::NoDevice => "No device",
        Key::Volume => "Volume",
        Key::Sync => "Sync",
        Key::Settings => "Settings",
        Key::Fullscreen => "Full screen",
        Key::Video => "Video",
        Key::Audio => "Audio",
        Key::ImageQuality => "Image quality",
        Key::PresetPerformance => "Performance",
        Key::PresetBalanced => "Balanced",
        Key::PresetHigh => "High",
        Key::Theme => "Theme",
        Key::Language => "Language",
        Key::Auto => "Auto",
        Key::Dark => "Dark",
        Key::Light => "Light",
        Key::English => "English",
        Key::Spanish => "Español",
        Key::Statistics => "Statistics",
        Key::Performance => "Performance",
        Key::Display => "Display",
        Key::Diagnosis => "Diagnosis",
        Key::DeviceMode => "Device mode",
        Key::Received => "Frames received",
        Key::Shown => "Frames shown",
        Key::Dropped => "Frames skipped",
        Key::AppLatency => "App latency",
        Key::Cpu => "CPU (this app)",
        Key::GpuPerFrame => "GPU per frame",
        Key::Memory => "Memory",
        Key::AudioBuffer => "Audio buffer",
        Key::Monitor => "Monitor",
        Key::WindowSize => "Window",
        Key::Bandwidth => "Mode data rate",
        Key::Measuring => "Measuring…",
        Key::FindHealthy => "Everything looks good.",
        Key::FindNoVideo => "No video device found. Check the cable and the capture card.",
        Key::FindVideoLost => "The capture card stopped responding. Trying to reconnect…",
        Key::FindNeedsUsb3 => "This mode needs a fast USB 3 port. Try a USB 3 port without a hub, or a lower mode.",
        Key::FindCaptureLow => "The device sends fewer frames than its mode promises. Low light (webcams), a busy USB port or the source itself can cause this.",
        Key::FindDropping => "Some frames are being skipped. Try the Performance image quality.",
        Key::FindHighCpu => "The app is using a lot of CPU. Close other heavy programs.",
        Key::FindSlowGpu => "Drawing takes a big part of each frame. Try the Performance image quality.",
        Key::FindAudioLost => "The audio device was lost. Choose it again in Settings.",
        Key::VideoMode => "Video mode",
        Key::ModeCompressed => "Compressed",
        Key::FindSwitched => "Switched to a compressed video mode because the USB port could not carry the uncompressed one. Use a USB 3 port and choose Auto in Settings to try again.",
        Key::InstantReplay => "Instant replay",
        Key::ReplayOff => "Off — turn it on to keep the last moments.",
        Key::ReplayRecording => "Recording",
        Key::ReplayStarting => "Starting…",
        Key::ReplayUnavailable => "Not available on this PC.",
        Key::ReplayKeep => "Keep the last",
        Key::ReplayHint => "A clip has at least this much. Hotkey: {hotkey}",
        Key::ReplayMemory => "Uses about {size} of memory.",
        Key::ReplayMemoryLimit => "Uses about {size} of memory. This is the longest this PC's memory allows.",
        Key::Microphone => "Microphone",
        Key::MicDefault => "Automatic",
        Key::MicHint => "Off until you turn it on. Saved as its own track.",
        Key::MicUnavailable => "Microphone not available",
        Key::ToastMicOn => "Microphone is on",
        Key::ToastMicOff => "Microphone is off",
        Key::ToastRecStarted => "Recording started",
        Key::ToastRecSaved => "Recording saved",
        Key::ToastRecFailed => "Could not record",
        Key::ToastStopRecordingFirst => "Stop the recording first",
        Key::ToastRecDiskFull => "Recording stopped: the disk is almost full",
        Key::ToastRecNewPart => "The picture changed size: recording continues in a new file",
        Key::ToastRecNoSignal => "There is no video signal to record",
        Key::ToastLowDisk => "Low disk space: the recording will stop if it fills up",
        Key::ToastRecNoSpace => "Not enough free disk space to record",
        Key::RecordingQuality => "Recording quality",
        Key::QualityNormal => "Normal",
        Key::QualityHigh => "High",
        Key::QualityHint => "A recording takes about {size} per hour.",
        Key::ChangeFolder => "Change…",
        Key::ToastFolderNotWritable => "Cannot save to that folder",
        Key::ToastQualityChanged => "Quality changed: the replay buffer starts over",
        Key::SourceCapture => "Capture card",
        Key::SourceScreen => "Screen",
        Key::SourceApp => "Application window",
        Key::SourceScreenWord => "Screen",
        Key::SourcePrimary => "primary",
        Key::ToastSourceLost => "The captured window closed: back to the capture card",
        Key::ToastSourceFailed => "That window cannot be captured: back to the capture card",
        Key::SaveClip => "Save clip",
        Key::OpenFolder => "Open folder",
        Key::ToastClipSaved => "Clip saved",
        Key::ToastClipFailed => "Could not save the clip",
        Key::ToastTurnOnFirst => "Turn on instant replay first",
        Key::ToastReplayOn => "Instant replay is on",
        Key::ToastReplayOff => "Instant replay is off",
        Key::ToastSaving => "Saving clip…",
    }
}

fn es(key: Key) -> &'static str {
    match key {
        Key::Live => "En vivo",
        Key::Connecting => "Conectando…",
        Key::Disconnected => "Desconectado",
        Key::NoDevice => "Sin dispositivo",
        Key::Volume => "Volumen",
        Key::Sync => "Sincronía",
        Key::Settings => "Ajustes",
        Key::Fullscreen => "Pantalla completa",
        Key::Video => "Video",
        Key::Audio => "Audio",
        Key::ImageQuality => "Calidad de imagen",
        Key::PresetPerformance => "Rendimiento",
        Key::PresetBalanced => "Equilibrado",
        Key::PresetHigh => "Alta",
        Key::Theme => "Tema",
        Key::Language => "Idioma",
        Key::Auto => "Automático",
        Key::Dark => "Oscuro",
        Key::Light => "Claro",
        Key::English => "English",
        Key::Spanish => "Español",
        Key::Statistics => "Estadísticas",
        Key::Performance => "Rendimiento",
        Key::Display => "Pantalla",
        Key::Diagnosis => "Diagnóstico",
        Key::DeviceMode => "Modo del dispositivo",
        Key::Received => "Frames recibidos",
        Key::Shown => "Frames mostrados",
        Key::Dropped => "Frames saltados",
        Key::AppLatency => "Latencia de la app",
        Key::Cpu => "CPU (esta app)",
        Key::GpuPerFrame => "GPU por frame",
        Key::Memory => "Memoria",
        Key::AudioBuffer => "Búfer de audio",
        Key::Monitor => "Monitor",
        Key::WindowSize => "Ventana",
        Key::Bandwidth => "Datos del modo",
        Key::Measuring => "Midiendo…",
        Key::FindHealthy => "Todo parece estar bien.",
        Key::FindNoVideo => "No se encontró ningún dispositivo de video. Revisa el cable y la capturadora.",
        Key::FindVideoLost => "La capturadora dejó de responder. Intentando reconectar…",
        Key::FindNeedsUsb3 => "Este modo necesita un puerto USB 3 rápido. Prueba un puerto USB 3 sin hub, o un modo menor.",
        Key::FindCaptureLow => "El dispositivo envía menos frames de los que anuncia. La poca luz (en webcams), un puerto USB saturado o la propia fuente pueden causarlo.",
        Key::FindDropping => "Se están saltando frames. Prueba la calidad de imagen «Rendimiento».",
        Key::FindHighCpu => "La app usa mucha CPU. Cierra otros programas pesados.",
        Key::FindSlowGpu => "Dibujar ocupa gran parte de cada frame. Prueba la calidad de imagen «Rendimiento».",
        Key::FindAudioLost => "Se perdió el dispositivo de audio. Vuelve a elegirlo en Ajustes.",
        Key::VideoMode => "Modo de video",
        Key::ModeCompressed => "Comprimido",
        Key::FindSwitched => "Se cambió a un modo de video comprimido porque el puerto USB no podía con el modo sin comprimir. Usa un puerto USB 3 y elige «Automático» en Ajustes para volver a probar.",
        Key::InstantReplay => "Repetición instantánea",
        Key::ReplayOff => "Apagada: actívala para guardar los últimos momentos.",
        Key::ReplayRecording => "Grabando",
        Key::ReplayStarting => "Iniciando…",
        Key::ReplayUnavailable => "No disponible en este PC.",
        Key::ReplayKeep => "Guardar los últimos",
        Key::ReplayHint => "Un clip tiene al menos este tiempo. Atajo: {hotkey}",
        Key::ReplayMemory => "Usa unos {size} de memoria.",
        Key::ReplayMemoryLimit => "Usa unos {size} de memoria. Es lo más largo que permite la memoria de este PC.",
        Key::Microphone => "Micrófono",
        Key::MicDefault => "Automático",
        Key::MicHint => "Apagado hasta que lo actives. Se guarda como pista propia.",
        Key::MicUnavailable => "Micrófono no disponible",
        Key::ToastMicOn => "Micrófono activado",
        Key::ToastMicOff => "Micrófono apagado",
        Key::ToastRecStarted => "Grabación iniciada",
        Key::ToastRecSaved => "Grabación guardada",
        Key::ToastRecFailed => "No se pudo grabar",
        Key::ToastStopRecordingFirst => "Detén primero la grabación",
        Key::ToastRecDiskFull => "Grabación detenida: el disco está casi lleno",
        Key::ToastRecNewPart => "La imagen cambió de tamaño: la grabación sigue en un archivo nuevo",
        Key::ToastRecNoSignal => "No hay señal de video para grabar",
        Key::ToastLowDisk => "Poco espacio en disco: la grabación se detendrá si se llena",
        Key::ToastRecNoSpace => "No hay espacio suficiente en el disco para grabar",
        Key::RecordingQuality => "Calidad de grabación",
        Key::QualityNormal => "Normal",
        Key::QualityHigh => "Alta",
        Key::QualityHint => "Una grabación ocupa unos {size} por hora.",
        Key::ChangeFolder => "Cambiar…",
        Key::ToastFolderNotWritable => "No se puede guardar en esa carpeta",
        Key::ToastQualityChanged => "Calidad cambiada: el búfer del replay empieza de nuevo",
        Key::SourceCapture => "Capturadora",
        Key::SourceScreen => "Pantalla",
        Key::SourceApp => "Ventana de aplicación",
        Key::SourceScreenWord => "Pantalla",
        Key::SourcePrimary => "principal",
        Key::ToastSourceLost => "La ventana capturada se cerró: de vuelta a la capturadora",
        Key::ToastSourceFailed => "No se puede capturar esa ventana: de vuelta a la capturadora",
        Key::SaveClip => "Guardar clip",
        Key::OpenFolder => "Abrir carpeta",
        Key::ToastClipSaved => "Clip guardado",
        Key::ToastClipFailed => "No se pudo guardar el clip",
        Key::ToastTurnOnFirst => "Activa primero la repetición instantánea",
        Key::ToastReplayOn => "Repetición instantánea activada",
        Key::ToastReplayOff => "Repetición instantánea desactivada",
        Key::ToastSaving => "Guardando clip…",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_has_a_non_empty_string_in_both_languages() {
        for key in ALL_KEYS {
            for lang in [Lang::En, Lang::Es] {
                assert!(!tr(lang, key).is_empty(), "{key:?} in {lang:?}");
            }
        }
    }

    #[test]
    fn all_keys_list_has_no_duplicates() {
        for (i, a) in ALL_KEYS.iter().enumerate() {
            assert!(!ALL_KEYS[i + 1..].contains(a), "{a:?} listed twice");
        }
    }

    #[test]
    fn spanish_and_english_actually_differ_where_expected() {
        assert_eq!(tr(Lang::En, Key::Settings), "Settings");
        assert_eq!(tr(Lang::Es, Key::Settings), "Ajustes");
    }

    #[test]
    fn durations_read_naturally() {
        assert_eq!(duration_label(5), "5 s");
        assert_eq!(duration_label(45), "45 s");
        assert_eq!(duration_label(60), "1 min");
        assert_eq!(duration_label(90), "1 min 30 s");
        assert_eq!(duration_label(600), "10 min");
        assert_eq!(duration_label(0), "0 s");
    }

    #[test]
    fn the_replay_hint_keeps_its_placeholder_in_both_languages() {
        for lang in [Lang::En, Lang::Es] {
            assert!(tr(lang, Key::ReplayHint).contains("{hotkey}"), "{lang:?}");
            assert!(tr(lang, Key::ReplayMemory).contains("{size}"), "{lang:?}");
            assert!(tr(lang, Key::QualityHint).contains("{size}"), "{lang:?}");
            assert!(
                tr(lang, Key::ReplayMemoryLimit).contains("{size}"),
                "{lang:?}"
            );
        }
    }

    #[test]
    fn any_spanish_langid_maps_to_spanish() {
        assert_eq!(Lang::from_langid(0x0C0A), Lang::Es); // es-ES (modern)
        assert_eq!(Lang::from_langid(0x080A), Lang::Es); // es-MX
        assert_eq!(Lang::from_langid(0x540A), Lang::Es); // es-US
        assert_eq!(Lang::from_langid(0x0409), Lang::En); // en-US
        assert_eq!(Lang::from_langid(0x0416), Lang::En); // pt-BR falls back
    }

    #[test]
    fn auto_follows_the_system_and_manual_overrides_it() {
        assert_eq!(LangMode::Auto.resolve(Lang::Es), Lang::Es);
        assert_eq!(LangMode::Auto.resolve(Lang::En), Lang::En);
        assert_eq!(LangMode::En.resolve(Lang::Es), Lang::En);
        assert_eq!(LangMode::Es.resolve(Lang::En), Lang::Es);
    }
}
