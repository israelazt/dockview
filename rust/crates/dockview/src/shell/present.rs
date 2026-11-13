//! How the application's events are shown: the words and the kind of toast.

use dockview_domain::events::AppEvent;
use dockview_domain::i18n::Key;
use dockview_ui::toast::ToastKind;

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
