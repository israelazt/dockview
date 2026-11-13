//! A model to draw in tests: a live capture card, nothing else going on.

use dockview_domain::i18n::Lang;
use dockview_domain::settings::{LangMode, RenderPreset, VideoModePref};
use dockview_domain::theme::{Theme, ThemeMode};
use dockview_domain::view::*;

pub(crate) fn model() -> Model {
    Model {
        theme: Theme::dark(),
        lang: Lang::En,
        status: StatusKind::Live,
        resolution: "1080p · 60 fps".into(),
        volume: 0.5,
        sync_ms: 0,
        fullscreen: false,
        video_devices: vec!["UGREEN 25173".into(), "Webcam".into()],
        video_selected: Some(0),
        audio_devices: vec!["HDMI (UGREEN 25173)".into()],
        audio_selected: Some(0),
        mic_devices: vec!["Windows default".into(), "USB Microphone".into()],
        mic_selected: Some(0),
        preset: RenderPreset::Balanced,
        theme_mode: ThemeMode::Auto,
        lang_mode: LangMode::Auto,
        video_mode: VideoModePref::Auto,
        stats_open: false,
        studio: false,
        replay: ReplayView::off("Ctrl+Alt+S"),
        mic: MicView::off(),
        rec: RecView::idle(),
        sources: SourcesView::default(),
        scenes: ScenesView::default(),
    }
}
