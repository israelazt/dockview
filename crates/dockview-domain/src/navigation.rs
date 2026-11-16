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
    /// Pictures the viewer lost per second, over the last second measured.
    pub drops_per_sec: f32,
    pub stream: StreamState,
}

/// Below this many lost pictures per second the figure reads as zero.
const DROPS_SHOWN_FROM: f32 = 0.05;

impl NavView {
    /// Pictures are being lost right now.
    pub fn is_dropping(&self) -> bool {
        self.drops_per_sec >= DROPS_SHOWN_FROM
    }

    /// The rate as written in the interface: `0`, `0.5` or `0,5`, `12`.
    pub fn drops_text(&self, lang: crate::i18n::Lang) -> String {
        let r = self.drops_per_sec;
        let text = if r < DROPS_SHOWN_FROM {
            "0".to_string()
        } else if r < 10.0 {
            format!("{r:.1}")
        } else {
            format!("{r:.0}")
        };
        match lang {
            crate::i18n::Lang::Es => text.replace('.', ","),
            _ => text,
        }
    }
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

#[cfg(test)]
mod drops_tests {
    use super::*;
    use crate::i18n::Lang;

    fn nav(rate: f32) -> NavView {
        NavView {
            drops_per_sec: rate,
            ..NavView::default()
        }
    }

    #[test]
    fn the_rate_is_written_the_way_the_language_does() {
        assert_eq!(nav(0.0).drops_text(Lang::En), "0");
        assert_eq!(nav(0.04).drops_text(Lang::En), "0", "too small to show");
        assert_eq!(nav(0.5).drops_text(Lang::En), "0.5");
        assert_eq!(nav(0.5).drops_text(Lang::Es), "0,5");
        assert_eq!(nav(12.4).drops_text(Lang::En), "12");
    }

    #[test]
    fn only_a_rate_that_shows_counts_as_dropping() {
        assert!(!nav(0.0).is_dropping() && !nav(0.04).is_dropping());
        assert!(nav(0.05).is_dropping() && nav(30.0).is_dropping());
    }
}
