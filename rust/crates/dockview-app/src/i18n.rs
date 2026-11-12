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
}

/// All keys, for completeness tests.
pub const ALL_KEYS: [Key; 50] = [
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
