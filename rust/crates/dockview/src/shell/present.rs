//! How the application's events are shown: the words and the kind of toast.

use std::path::Path;

use dockview_domain::events::AppEvent;
use dockview_domain::i18n::{self, Key, Lang};
use dockview_ui::toast::ToastKind;
use windows::core::HSTRING;
use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};

/// Why the program could not start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupFailure {
    /// The window (or the event loop behind it) could not be created.
    Window,
    /// Direct3D could not be started.
    Graphics,
}

/// What the box says when the program cannot start: the reason in the user's
/// words, then the technical detail and where the log is.
pub fn startup_failure_text(
    lang: Lang,
    failure: StartupFailure,
    detail: &str,
    log: Option<&Path>,
) -> String {
    let reason = match failure {
        StartupFailure::Window => Key::StartupFailedWindow,
        StartupFailure::Graphics => Key::StartupFailedGraphics,
    };
    let mut text = format!("{}\n\n{detail}", i18n::tr(lang, reason));
    if let Some(log) = log {
        text.push_str(&format!(
            "\n\n{}\n{}",
            i18n::tr(lang, Key::StartupFailedLog),
            log.display()
        ));
    }
    text
}

/// Tells the user the program cannot start. With no window to show it in, a
/// message box does it; `silent` (a scripted run) skips the box, because
/// nobody is there to close it and the exit code and the log say it already.
pub fn report_startup_failure(lang: Lang, failure: StartupFailure, detail: &str, silent: bool) {
    if silent {
        return;
    }
    let log = dockview_platform::log_file::default_log_path();
    let text = startup_failure_text(lang, failure, detail, log.as_deref());
    let title = i18n::tr(lang, Key::StartupFailedTitle);
    // SAFETY: a plain call with strings that outlive it and no owner window.
    unsafe {
        MessageBoxW(
            None,
            &HSTRING::from(text),
            &HSTRING::from(title),
            MB_OK | MB_ICONERROR,
        );
    }
}

/// How an event is shown: the words and the kind of toast.
pub fn toast_for(event: AppEvent) -> (Key, ToastKind) {
    match event {
        AppEvent::ReplayIsOff => (Key::ToastTurnOnFirst, ToastKind::Info),
        AppEvent::ClipSaving => (Key::ToastSaving, ToastKind::Info),
        AppEvent::ClipSaved => (Key::ToastClipSaved, ToastKind::Success),
        AppEvent::ClipFailed => (Key::ToastClipFailed, ToastKind::Error),
        AppEvent::RecordingStarted => (Key::ToastRecStarted, ToastKind::Info),
        AppEvent::RecordingFailed => (Key::ToastRecFailed, ToastKind::Error),
        AppEvent::RecordingNoSpace => (Key::ToastRecNoSpace, ToastKind::Error),
        AppEvent::RecordingSaved => (Key::ToastRecSaved, ToastKind::Success),
        AppEvent::RecordingNewPart => (Key::ToastRecNewPart, ToastKind::Info),
        AppEvent::RecordingDiskFull => (Key::ToastRecDiskFull, ToastKind::Error),
        AppEvent::RecordingNoSignal => (Key::ToastRecNoSignal, ToastKind::Error),
        AppEvent::LowDisk => (Key::ToastLowDisk, ToastKind::Error),
        AppEvent::StopRecordingFirst => (Key::ToastStopRecordingFirst, ToastKind::Info),
        AppEvent::ReplayOn => (Key::ToastReplayOn, ToastKind::Info),
        AppEvent::ReplayOff => (Key::ToastReplayOff, ToastKind::Info),
        AppEvent::MicOn => (Key::ToastMicOn, ToastKind::Info),
        AppEvent::MicOff => (Key::ToastMicOff, ToastKind::Info),
        AppEvent::QualityChanged => (Key::ToastQualityChanged, ToastKind::Info),
        AppEvent::FolderNotWritable => (Key::ToastFolderNotWritable, ToastKind::Error),
        AppEvent::SceneMissing => (Key::ToastSceneMissing, ToastKind::Error),
        AppEvent::PipLost => (Key::ToastPipLost, ToastKind::Error),
        AppEvent::SourceLost => (Key::ToastSourceLost, ToastKind::Error),
        AppEvent::SourceFailed => (Key::ToastSourceFailed, ToastKind::Error),
        AppEvent::AudioLost => (Key::ToastAudioLost, ToastKind::Error),
        AppEvent::AudioBack => (Key::ToastAudioBack, ToastKind::Success),
        AppEvent::SettingsNotSaved => (Key::ToastSettingsNotSaved, ToastKind::Error),
    }
}

#[cfg(test)]
mod tests {
    use dockview_domain::i18n;

    use super::*;

    #[test]
    fn the_start_up_failure_box_says_why_gives_the_detail_and_points_to_the_log() {
        let log = Path::new(r"C:\Users\me\AppData\Roaming\Dockview\dockview.log");
        let en = startup_failure_text(Lang::En, StartupFailure::Graphics, "E_FAIL", Some(log));
        assert!(en.contains("Direct3D") && en.contains("E_FAIL") && en.contains("dockview.log"));
        let es = startup_failure_text(Lang::Es, StartupFailure::Window, "no handle", None);
        assert!(es.contains("ventana") && es.contains("no handle"));
        assert!(!es.contains("registro"), "no log line without a log");
    }

    #[test]
    fn every_event_has_a_message_and_failures_are_errors() {
        use AppEvent::*;
        for event in [
            ReplayIsOff,
            ClipSaving,
            ClipSaved,
            ClipFailed,
            RecordingStarted,
            RecordingFailed,
            RecordingNoSpace,
            RecordingSaved,
            RecordingNewPart,
            RecordingDiskFull,
            RecordingNoSignal,
            LowDisk,
            StopRecordingFirst,
            ReplayOn,
            ReplayOff,
            MicOn,
            MicOff,
            QualityChanged,
            FolderNotWritable,
            SceneMissing,
            PipLost,
            SourceLost,
            SourceFailed,
            AudioLost,
            AudioBack,
            SettingsNotSaved,
        ] {
            let (key, _) = toast_for(event);
            assert!(!i18n::tr(i18n::Lang::En, key).is_empty(), "{event:?}");
        }
        assert_eq!(toast_for(ClipSaved).1, ToastKind::Success);
        assert_eq!(toast_for(ClipFailed).1, ToastKind::Error);
        assert_eq!(toast_for(LowDisk).1, ToastKind::Error);
        assert_eq!(toast_for(AudioLost).1, ToastKind::Error);
        assert_eq!(toast_for(AudioBack).1, ToastKind::Success);
    }
}
