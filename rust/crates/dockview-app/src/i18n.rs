//! UI strings in American English and Latin American Spanish.

/// Languages the interface is available in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    En,
    Es,
}

pub use dockview_platform::settings::LangMode;

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

/// Declares every translatable string once, with both texts, and builds from
/// that the [`Key`] enum, [`ALL_KEYS`] and the two lookups. A key cannot exist
/// without both translations, and the list cannot fall out of step with the
/// enum, because there is nothing to keep in step.
macro_rules! strings {
    ($($key:ident => $en:literal, $es:literal;)*) => {
        /// Every translatable string.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum Key {
            $($key),*
        }

        /// All keys, for completeness tests.
        pub const ALL_KEYS: [Key; [$(Key::$key),*].len()] = [$(Key::$key),*];

        fn en(key: Key) -> &'static str {
            match key {
                $(Key::$key => $en),*
            }
        }

        fn es(key: Key) -> &'static str {
            match key {
                $(Key::$key => $es),*
            }
        }
    };
}

strings! {
    Live => "Live", "En vivo";
    Connecting => "Connecting…", "Conectando…";
    Disconnected => "Disconnected", "Desconectado";
    NoDevice => "No device", "Sin dispositivo";
    Volume => "Volume", "Volumen";
    Sync => "Sync", "Sincronía";
    Settings => "Settings", "Ajustes";
    Fullscreen => "Full screen", "Pantalla completa";
    Video => "Video", "Video";
    Audio => "Audio", "Audio";
    ImageQuality => "Image quality", "Calidad de imagen";
    PresetPerformance => "Performance", "Rendimiento";
    PresetBalanced => "Balanced", "Equilibrado";
    PresetHigh => "High", "Alta";
    Theme => "Theme", "Tema";
    Language => "Language", "Idioma";
    Auto => "Auto", "Automático";
    Dark => "Dark", "Oscuro";
    Light => "Light", "Claro";
    English => "English", "English";
    Spanish => "Español", "Español";
    Statistics => "Statistics", "Estadísticas";
    Performance => "Performance", "Rendimiento";
    Display => "Display", "Pantalla";
    Diagnosis => "Diagnosis", "Diagnóstico";
    DeviceMode => "Device mode", "Modo del dispositivo";
    Received => "Frames received", "Frames recibidos";
    Shown => "Frames shown", "Frames mostrados";
    Dropped => "Frames skipped", "Frames saltados";
    AppLatency => "App latency", "Latencia de la app";
    Cpu => "CPU (this app)", "CPU (esta app)";
    GpuPerFrame => "GPU per frame", "GPU por frame";
    Memory => "Memory", "Memoria";
    AudioBuffer => "Audio buffer", "Búfer de audio";
    Monitor => "Monitor", "Monitor";
    WindowSize => "Window", "Ventana";
    Bandwidth => "Mode data rate", "Datos del modo";
    Measuring => "Measuring…", "Midiendo…";
    FindHealthy => "Everything looks good.", "Todo parece estar bien.";
    FindNoVideo => "No video device found. Check the cable and the capture card.", "No se encontró ningún dispositivo de video. Revisa el cable y la capturadora.";
    FindVideoLost => "The capture card stopped responding. Trying to reconnect…", "La capturadora dejó de responder. Intentando reconectar…";
    FindNeedsUsb3 => "This mode needs a fast USB 3 port. Try a USB 3 port without a hub, or a lower mode.", "Este modo necesita un puerto USB 3 rápido. Prueba un puerto USB 3 sin hub, o un modo menor.";
    FindCaptureLow => "The device sends fewer frames than its mode promises. Low light (webcams), a busy USB port or the source itself can cause this.", "El dispositivo envía menos frames de los que anuncia. La poca luz (en webcams), un puerto USB saturado o la propia fuente pueden causarlo.";
    FindDropping => "Some frames are being skipped. Try the Performance image quality.", "Se están saltando frames. Prueba la calidad de imagen «Rendimiento».";
    FindHighCpu => "The app is using a lot of CPU. Close other heavy programs.", "La app usa mucha CPU. Cierra otros programas pesados.";
    FindSlowGpu => "Drawing takes a big part of each frame. Try the Performance image quality.", "Dibujar ocupa gran parte de cada frame. Prueba la calidad de imagen «Rendimiento».";
    FindAudioLost => "The audio device was lost. Choose it again in Settings.", "Se perdió el dispositivo de audio. Vuelve a elegirlo en Ajustes.";
    VideoMode => "Video mode", "Modo de video";
    ModeCompressed => "Compressed", "Comprimido";
    FindSwitched => "Switched to a compressed video mode because the USB port could not carry the uncompressed one. Use a USB 3 port and choose Auto in Settings to try again.", "Se cambió a un modo de video comprimido porque el puerto USB no podía con el modo sin comprimir. Usa un puerto USB 3 y elige «Automático» en Ajustes para volver a probar.";
    InstantReplay => "Instant replay", "Repetición instantánea";
    ReplayOff => "Off — turn it on to keep the last moments.", "Apagada: actívala para guardar los últimos momentos.";
    ReplayRecording => "Recording", "Grabando";
    ReplayStarting => "Starting…", "Iniciando…";
    ReplayUnavailable => "Not available on this PC.", "No disponible en este PC.";
    ReplayKeep => "Keep the last", "Guardar los últimos";
    ReplayHint => "A clip has at least this much. Hotkey: {hotkey}", "Un clip tiene al menos este tiempo. Atajo: {hotkey}";
    ReplayMemory => "Uses about {size} of memory.", "Usa unos {size} de memoria.";
    ReplayMemoryLimit => "Uses about {size} of memory. This is the longest this PC's memory allows.", "Usa unos {size} de memoria. Es lo más largo que permite la memoria de este PC.";
    Microphone => "Microphone", "Micrófono";
    MicDefault => "Automatic", "Automático";
    MicHint => "Off. Saved as its own track.", "Apagado. Se guarda como pista propia.";
    MicUnavailable => "Microphone not available", "Micrófono no disponible";
    ToastMicOn => "Microphone is on", "Micrófono activado";
    ToastMicOff => "Microphone is off", "Micrófono apagado";
    ToastRecStarted => "Recording started", "Grabación iniciada";
    ToastRecSaved => "Recording saved", "Grabación guardada";
    ToastRecFailed => "Could not record", "No se pudo grabar";
    ToastStopRecordingFirst => "Stop the recording first", "Detén primero la grabación";
    ToastRecDiskFull => "Recording stopped: the disk is almost full", "Grabación detenida: el disco está casi lleno";
    ToastRecNewPart => "The picture changed size: recording continues in a new file", "La imagen cambió de tamaño: la grabación sigue en un archivo nuevo";
    ToastRecNoSignal => "There is no video signal to record", "No hay señal de video para grabar";
    ToastLowDisk => "Low disk space: the recording will stop if it fills up", "Poco espacio en disco: la grabación se detendrá si se llena";
    ToastRecNoSpace => "Not enough free disk space to record", "No hay espacio suficiente en el disco para grabar";
    RecordingQuality => "Recording quality", "Calidad de grabación";
    QualityNormal => "Normal", "Normal";
    QualityHigh => "High", "Alta";
    QualityHint => "A recording takes about {size} per hour.", "Una grabación ocupa unos {size} por hora.";
    ChangeFolder => "Change…", "Cambiar…";
    ToastFolderNotWritable => "Cannot save to that folder", "No se puede guardar en esa carpeta";
    ToastQualityChanged => "Quality changed: the replay buffer starts over", "Calidad cambiada: el búfer del replay empieza de nuevo";
    SourceCapture => "Capture card", "Capturadora";
    SourceScreen => "Screen", "Pantalla";
    SourceApp => "Application window", "Ventana de aplicación";
    SourceScreenWord => "Screen", "Pantalla";
    SourcePrimary => "primary", "principal";
    ToastSourceLost => "The captured window closed: back to the capture card", "La ventana capturada se cerró: de vuelta a la capturadora";
    ToastSourceFailed => "That window cannot be captured: back to the capture card", "No se puede capturar esa ventana: de vuelta a la capturadora";
    ToastPipLost => "The corner source stopped and was taken off the picture", "La fuente de la esquina se detuvo y se quitó de la imagen";
    ToastSceneMissing => "Part of the scene is not available right now", "Una parte de la escena no está disponible ahora";
    ScenesSection => "Scenes", "Escenas";
    ScenePlain => "Single source", "Fuente simple";
    SceneNew => "New scene", "Nueva escena";
    SceneLayers => "Layers", "Capas";
    SceneAddSource => "Add a source", "Añadir una fuente";
    SceneAddTitle => "Add to the scene", "Añadir a la escena";
    SceneBaseTag => "base", "base";
    SceneDefaultName => "Scene {n}", "Escena {n}";
    TipScenes => "Scenes: arrange several sources in one picture", "Escenas: combina varias fuentes en una sola imagen";
    TipScenePlain => "Show just the selected source", "Mostrar solo la fuente elegida";
    TipSceneRow => "Switch to this scene", "Cambiar a esta escena";
    TipSceneDelete => "Delete this scene", "Borrar esta escena";
    TipSceneNew => "Make a scene from the current source", "Crear una escena con la fuente actual";
    TipLayerHide => "Hide this layer", "Ocultar esta capa";
    TipLayerShow => "Show this layer", "Mostrar esta capa";
    TipLayerRemove => "Remove this layer from the scene", "Quitar esta capa de la escena";
    TipSceneAdd => "Add another source on top", "Añadir otra fuente encima";
    TipSceneBack => "Back to the scenes", "Volver a las escenas";
    TipSources => "Choose what to capture: a capture card, a screen or an application", "Elegir qué capturar: una capturadora, una pantalla o una aplicación";
    TipReplay => "Instant replay options", "Opciones de la repetición instantánea";
    TipSave => "Save the last {seconds} as a clip ({hotkey})", "Guardar los últimos {seconds} como clip ({hotkey})";
    TipSaveOff => "Instant replay is off: turn it on to save clips", "La repetición instantánea está apagada: actívala para guardar clips";
    TipRecord => "Start recording ({hotkey})", "Empezar a grabar ({hotkey})";
    TipRecordStop => "Stop recording ({hotkey})", "Detener la grabación ({hotkey})";
    TipRecordCancel => "Cancel: still waiting to start ({hotkey})", "Cancelar: aún esperando para empezar ({hotkey})";
    TipSettings => "Settings: devices, image and language", "Ajustes: dispositivos, imagen e idioma";
    TipFullscreen => "Fullscreen (F)", "Pantalla completa (F)";
    TipExitFullscreen => "Leave fullscreen (Esc)", "Salir de pantalla completa (Esc)";
    TipVolume => "Volume (↑ ↓ or the mouse wheel)", "Volumen (↑ ↓ o la rueda del ratón)";
    TipSyncEarlier => "Play the sound a little earlier (←)", "Adelantar un poco el sonido (←)";
    TipSyncLater => "Delay the sound a little (→)", "Retrasar un poco el sonido (→)";
    TipReplayToggle => "Turn the instant replay on or off", "Encender o apagar la repetición instantánea";
    TipReplayShorter => "Keep less time: shorter clips and less memory", "Guardar menos tiempo: clips más cortos y menos memoria";
    TipReplayLonger => "Keep more time: longer clips and more memory", "Guardar más tiempo: clips más largos y más memoria";
    TipMicToggle => "Turn the microphone on or off. It is saved as its own track", "Encender o apagar el micrófono. Se guarda como pista propia";
    TipFolderOpen => "Show the last saved file, or open the folder", "Mostrar el último archivo guardado, o abrir la carpeta";
    TipFolderChange => "Choose another folder for clips and recordings", "Elegir otra carpeta para clips y grabaciones";
    TipQualityNormal => "Smaller files, still good for sharing", "Archivos más pequeños, buenos para compartir";
    TipQualityHigh => "The best picture, with big files", "La mejor imagen, con archivos grandes";
    TipVideoAuto => "Best quality; switches to a lighter mode by itself if the USB port is slow", "La mejor calidad; cambia solo a un modo más ligero si el puerto USB es lento";
    TipVideoCompressed => "Always use the lighter compressed mode", "Usar siempre el modo comprimido, más ligero";
    TipStats => "Show statistics and a plain-language diagnosis (I)", "Mostrar estadísticas y un diagnóstico en lenguaje sencillo (I)";
    TipPresetPerformance => "Fastest drawing, lowest use of the PC", "El dibujo más rápido, con el menor uso del PC";
    TipPresetBalanced => "A good balance of speed and sharpness", "Un buen equilibrio entre velocidad y nitidez";
    TipPresetQuality => "The sharpest image, using more of the graphics card", "La imagen más nítida, usando más la tarjeta gráfica";
    SaveClip => "Save clip", "Guardar clip";
    OpenFolder => "Open folder", "Abrir carpeta";
    ToastClipSaved => "Clip saved", "Clip guardado";
    ToastClipFailed => "Could not save the clip", "No se pudo guardar el clip";
    ToastTurnOnFirst => "Turn on instant replay first", "Activa primero la repetición instantánea";
    ToastReplayOn => "Instant replay is on", "Repetición instantánea activada";
    ToastReplayOff => "Instant replay is off", "Repetición instantánea desactivada";
    ToastSaving => "Saving clip…", "Guardando clip…";
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

/// Looks up `key` in `lang`.
pub fn tr(lang: Lang, key: Key) -> &'static str {
    match lang {
        Lang::En => en(key),
        Lang::Es => es(key),
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

    #[test]
    fn every_key_is_listed_once_and_in_order() {
        // `ALL_KEYS` is built from the same declaration as the enum, so the
        // first key is the first one declared and the count matches.
        assert_eq!(ALL_KEYS[0], Key::Live);
        assert_eq!(ALL_KEYS.last(), Some(&Key::ToastSaving));
        assert!(ALL_KEYS.len() > 100);
    }
}
