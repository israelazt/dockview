//! The navigation row under the title bar: the four tabs and the status on its
//! right. Plain data and the words for it; what draws it is `dockview-ui`.

use crate::broadcast::StreamState;
use crate::i18n::Key;

/// The tabs, left to right.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum NavTab {
    /// The picture, with the controls over it (the Studio panel closed).
    #[default]
    Viewer,
    Studio,
    Recordings,
    Settings,
}

impl NavTab {
    pub const ALL: [NavTab; 4] = [Self::Viewer, Self::Studio, Self::Recordings, Self::Settings];

    /// The words on the tab.
    pub fn title(self) -> Key {
        match self {
            Self::Viewer => Key::NavViewer,
            Self::Studio => Key::NavStudio,
            Self::Recordings => Key::NavRecordings,
            Self::Settings => Key::NavSettings,
        }
    }

    /// Whether the tab shows the Studio panel.
    pub fn opens_panel(self) -> bool {
        self != Self::Viewer
    }
}

/// What the video encoder is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EncoderState {
    /// Not known yet (it is being asked): nothing is shown.
    #[default]
    Unknown,
    /// There is no hardware encoder on this PC.
    Unavailable,
    /// There is one and nothing is using it.
    Ready,
    /// The replay, a recording or the stream is encoding.
    Active,
}

/// What the machine says about its hardware video encoder.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum EncoderInfo {
    /// Still being looked for.
    #[default]
    Checking,
    /// There is none.
    Missing,
    /// There is one, under this name (as Windows lists it).
    Found(String),
}

/// What the status on the right of the row shows.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NavView {
    /// The tab that is on: `Viewer` while the panel is closed.
    pub tab: NavTab,
    pub encoder: EncoderState,
    /// The short name of the encoder (see [`encoder_badge`]); empty when unknown.
    pub encoder_name: String,
    /// Pictures lost this session.
    pub drops: u64,
    pub stream: StreamState,
}

/// The short name of a hardware encoder, from the name Windows gives it, in
/// the way people say it: `NVENC`, `QSV`, `AMF`. Anything else is `H.264`.
pub fn encoder_badge(friendly_name: &str) -> &'static str {
    let name = friendly_name.to_ascii_lowercase();
    if name.contains("nvidia") || name.contains("nvenc") {
        "NVENC"
    } else if name.contains("intel") || name.contains("quick sync") || name.contains("qsv") {
        "QSV"
    } else if name.contains("amd") || name.contains("advanced micro") || name.contains("amf") {
        "AMF"
    } else {
        "H.264"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tabs_are_in_order_and_only_the_viewer_has_no_panel() {
        assert_eq!(NavTab::ALL[0], NavTab::Viewer);
        assert!(!NavTab::Viewer.opens_panel());
        assert!(NavTab::ALL[1..].iter().all(|t| t.opens_panel()));
        assert_eq!(NavTab::default(), NavTab::Viewer);
    }

    #[test]
    fn encoders_get_the_name_people_use() {
        assert_eq!(encoder_badge("NVIDIA H.264 Encoder MFT"), "NVENC");
        assert_eq!(
            encoder_badge("Intel® Quick Sync Video H.264 Encoder MFT"),
            "QSV"
        );
        assert_eq!(encoder_badge("AMDh264Encoder"), "AMF");
        assert_eq!(encoder_badge("AMD Radeon H264 Encoder"), "AMF");
        assert_eq!(encoder_badge("Some Other Encoder"), "H.264");
        assert_eq!(encoder_badge(""), "H.264");
    }
}
