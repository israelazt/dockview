//! Capture device descriptors and the audio↔video matching heuristic.

/// Minimum length (in characters) a word must exceed to count as significant.
const MIN_WORD_LEN: usize = 3;

/// A video capture device discovered through Media Foundation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoDevice {
    pub friendly_name: String,
    /// Symbolic link used to re-open the device (e.g. `\\?\usb#vid_...`).
    pub symbolic_link: String,
}

/// An audio capture endpoint discovered through WASAPI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioDevice {
    pub friendly_name: String,
    /// WASAPI endpoint id (e.g. `{0.0.1.00000000}.{guid}`).
    pub device_id: String,
}

/// Finds the first audio device whose name shares a significant word with the
/// video device (for example both contain `UGREEN`).
///
/// A word is significant when it has more than three characters, so generic
/// tokens such as `USB` never produce a match. Comparison is case-insensitive
/// and matches substrings. Returns `None` when there is no video device or no
/// audio device matches.
pub fn match_audio_to_video<'a>(
    video: Option<&VideoDevice>,
    audio: &'a [AudioDevice],
) -> Option<&'a AudioDevice> {
    let video = video?;

    let words: Vec<String> = video
        .friendly_name
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() > MIN_WORD_LEN)
        .map(str::to_lowercase)
        .collect();

    audio.iter().find(|a| {
        let name = a.friendly_name.to_lowercase();
        words.iter().any(|w| name.contains(w.as_str()))
    })
}

const WEBCAM_WORDS: [&str; 4] = ["webcam", "camera", "cámara", "camara"];

/// Whether `audio` is the sound of one of the `videos` that is a capture card
/// (not a webcam, whose own microphone is a fair fallback). Cheap cards name
/// their audio after the video device ("C7 USB3.0 Audio") with none of the
/// words that give a capture card away, so it is found by name instead.
fn is_capture_card_audio(audio: &AudioDevice, videos: &[VideoDevice]) -> bool {
    videos.iter().any(|video| {
        let name = video.friendly_name.to_lowercase();
        !WEBCAM_WORDS.iter().any(|w| name.contains(w))
            && match_audio_to_video(Some(video), std::slice::from_ref(audio)).is_some()
    })
}

/// How much an input looks like a real microphone, judged by its name. Lower
/// is better.
fn mic_rank(name: &str) -> u8 {
    let name = name.to_lowercase();
    // Capture cards' audio, and "what you hear" devices: never a microphone.
    const NOT_A_MIC: [&str; 10] = [
        "hdmi",
        "capture",
        "captura",
        "ugreen",
        "elgato",
        "avermedia",
        "magewell",
        "cam link",
        "stereo mix",
        "mezcla est",
    ];
    if NOT_A_MIC.iter().any(|w| name.contains(w)) {
        2
    } else if WEBCAM_WORDS.iter().any(|w| name.contains(w)) {
        1
    } else {
        0
    }
}

/// Chooses the microphone to use when the user has not picked one.
///
/// The Windows default input is the natural choice, but on a PC with a capture
/// card it is often the card's own HDMI audio, which is silence as a
/// microphone. So the default is used unless it looks like a capture card (or
/// is `exclude`, the device already used for the game's sound); then the best
/// remaining input wins: a real microphone before a webcam's. Returns `None`
/// when there is nothing better than a capture card.
///
/// `videos` are all the video devices present, so the audio of every capture
/// card is skipped, not just the one in use (with two cards plugged in, the
/// other one's audio can be Windows' default input).
pub fn pick_microphone<'a>(
    devices: &'a [AudioDevice],
    windows_default: Option<&'a AudioDevice>,
    exclude: Option<&AudioDevice>,
    videos: &[VideoDevice],
) -> Option<&'a AudioDevice> {
    let allowed = |d: &AudioDevice| {
        exclude.is_none_or(|e| e.device_id != d.device_id)
            && mic_rank(&d.friendly_name) < 2
            && !is_capture_card_audio(d, videos)
    };
    if let Some(default) = windows_default {
        if allowed(default) {
            return Some(default);
        }
    }
    devices
        .iter()
        .filter(|d| allowed(d))
        .min_by_key(|d| mic_rank(&d.friendly_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn video(name: &str) -> VideoDevice {
        VideoDevice {
            friendly_name: name.into(),
            symbolic_link: r"\\?\usb#test".into(),
        }
    }

    fn audio(name: &str) -> AudioDevice {
        AudioDevice {
            friendly_name: name.into(),
            device_id: "test-id".into(),
        }
    }

    #[test]
    fn no_video_returns_none() {
        let devices = [audio("HDMI (UGREEN 25173)")];
        assert_eq!(match_audio_to_video(None, &devices), None);
    }

    #[test]
    fn matches_by_shared_word_longer_than_three_chars() {
        let devices = [audio("Speakers (Realtek)"), audio("HDMI (UGREEN 25173)")];
        let result = match_audio_to_video(Some(&video("UGREEN 25173")), &devices);
        assert_eq!(result, Some(&devices[1]));
    }

    #[test]
    fn short_words_do_not_match() {
        // "USB" has 3 chars and "Cap" is shorter: nothing significant is shared.
        let devices = [audio("USB Audio")];
        assert_eq!(
            match_audio_to_video(Some(&video("USB Cap")), &devices),
            None
        );
    }

    #[test]
    fn match_is_case_insensitive() {
        let devices = [audio("HDMI (UGREEN)")];
        let result = match_audio_to_video(Some(&video("ugreen capture")), &devices);
        assert_eq!(result, Some(&devices[0]));
    }

    #[test]
    fn empty_audio_list_returns_none() {
        assert_eq!(
            match_audio_to_video(Some(&video("UGREEN 25173")), &[]),
            None
        );
    }

    #[test]
    fn punctuation_around_words_is_ignored() {
        // The C# version kept "(UGREEN)" as one token and failed to match.
        let devices = [audio("HDMI UGREEN Audio")];
        let result = match_audio_to_video(Some(&video("USB Video (UGREEN)")), &devices);
        assert_eq!(result, Some(&devices[0]));
    }

    #[test]
    fn first_matching_device_wins() {
        let devices = [audio("UGREEN Mic"), audio("UGREEN HDMI")];
        let result = match_audio_to_video(Some(&video("UGREEN 25173")), &devices);
        assert_eq!(result, Some(&devices[0]));
    }

    fn input(name: &str, id: &str) -> AudioDevice {
        AudioDevice {
            friendly_name: name.into(),
            device_id: id.into(),
        }
    }

    #[test]
    fn a_capture_cards_audio_is_never_taken_for_a_microphone() {
        // This is the real list of the author's PC: the default input is the
        // capture card.
        let card = input("HDMI (UGREEN 25173)", "card");
        let webcam = input("Micrófono (GENERAL WEBCAM)", "cam");
        let rode = input("Desktop Microphone (RØDE NT-USB Mini)", "rode");
        let all = [card.clone(), webcam.clone(), rode.clone()];
        assert_eq!(
            pick_microphone(&all, Some(&card), None, &[]),
            Some(&all[2]),
            "a real mic beats a webcam"
        );
    }

    #[test]
    fn a_sensible_windows_default_is_respected() {
        let card = input("HDMI (UGREEN 25173)", "card");
        let webcam = input("Micrófono (GENERAL WEBCAM)", "cam");
        let rode = input("Desktop Microphone (RØDE NT-USB Mini)", "rode");
        let all = [card, webcam, rode];
        // The user chose the webcam as their default: that is their call.
        assert_eq!(
            pick_microphone(&all, Some(&all[1]), None, &[]),
            Some(&all[1])
        );
        assert_eq!(
            pick_microphone(&all, Some(&all[2]), None, &[]),
            Some(&all[2])
        );
    }

    #[test]
    fn the_device_used_for_the_games_sound_is_excluded() {
        let headset = input("Headset Microphone", "headset");
        let other = input("Line In", "line");
        let all = [headset.clone(), other];
        assert_eq!(
            pick_microphone(&all, Some(&headset), Some(&headset), &[]),
            Some(&all[1])
        );
    }

    #[test]
    fn the_audio_of_a_second_capture_card_is_never_the_microphone() {
        // Two cards at once: Windows made the cheap one's audio the default
        // input, and its name has no capture-card keyword.
        let c7 = input("Interfaz de sonido digital (2- C7 USB3.0 Audio)", "c7");
        let ugreen = input("HDMI (UGREEN 25173)", "ugreen");
        let rode = input("Desktop Microphone (RØDE NT-USB Mini)", "rode");
        let all = [c7.clone(), ugreen, rode];
        let videos = [
            video("C7 USB3.0 Video"),
            video("UGREEN 25173"),
            video("GENERAL WEBCAM"),
        ];
        assert_eq!(
            pick_microphone(&all, Some(&c7), None, &videos),
            Some(&all[2])
        );
        // Without the video list the card could not be told apart.
        assert_eq!(pick_microphone(&all, Some(&c7), None, &[]), Some(&all[0]));
    }

    #[test]
    fn a_webcams_microphone_is_still_a_fair_fallback() {
        let c7 = input("Interfaz de sonido digital (2- C7 USB3.0 Audio)", "c7");
        let webcam = input("Micrófono (GENERAL WEBCAM)", "cam");
        let all = [c7.clone(), webcam];
        let videos = [video("C7 USB3.0 Video"), video("GENERAL WEBCAM")];
        assert_eq!(
            pick_microphone(&all, Some(&c7), None, &videos),
            Some(&all[1])
        );
    }

    #[test]
    fn with_only_capture_cards_there_is_no_microphone() {
        let card = input("HDMI (UGREEN 25173)", "card");
        let mix = input("Stereo Mix (Realtek)", "mix");
        let all = [card.clone(), mix];
        assert_eq!(pick_microphone(&all, Some(&card), None, &[]), None);
        assert_eq!(pick_microphone(&[], None, None, &[]), None);
    }

    #[test]
    fn exactly_four_chars_is_significant() {
        let devices = [audio("Elgato HD60")];
        let result = match_audio_to_video(Some(&video("HD60 S")), &devices);
        assert_eq!(result, Some(&devices[0]));
    }
}
