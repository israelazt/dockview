//! What the Stream section says about the stream: the sentence about what it
//! is doing, how the connection is, and what the platform recommends for the
//! chosen bitrate. Pure choices of words (and how tall they are), kept apart
//! from the drawing.

use dockview_domain::broadcast::StreamState;
use dockview_domain::i18n::{tr, Key};
use dockview_domain::stream_advice::{fit, Fit, Platform};
use dockview_domain::stream_health::StreamHealth;
use dockview_domain::view::Model;

use super::stream::{HINT, HINT_LINE};
use crate::stats_data::wrap_text;
use crate::Ui;

/// What the platform recommends for the chosen bitrate, and whether the
/// choice is within it; `None` for a service we have no advice for. The
/// verdict on the choice is a sentence apart, only when it is not fine.
pub(super) fn advice_sentence(m: &Model) -> Option<(String, Option<String>, Fit)> {
    let (s, lang) = (&m.stream, m.lang);
    let platform = Platform::of_address(&s.address)?;
    let (min, max) = platform.recommended_kbps(s.fps);
    let text = tr(lang, Key::StreamAdvice)
        .replace("{service}", platform.name())
        .replace("{min}", &min.to_string())
        .replace("{max}", &max.to_string());
    let verdict = fit(platform, s.fps, s.video_kbps);
    let remark = match verdict {
        Fit::Low => Some(tr(lang, Key::StreamAdviceLow).to_owned()),
        Fit::High => Some(tr(lang, Key::StreamAdviceHigh).to_owned()),
        Fit::Fine => None,
    };
    Some((text, remark, verdict))
}

/// The most lines the remark on the choice may take.
pub(super) const REMARK_LINES: usize = 2;

/// How tall the advice is: its line and the lines of the remark, if any.
pub(super) fn advice_height(ui: &mut Ui, m: &Model, width: f32) -> f32 {
    let Some((_, remark, _)) = advice_sentence(m) else {
        return 0.0;
    };
    let lines = remark.map_or(0, |r| {
        wrap_text(ui, &r, HINT, width).len().min(REMARK_LINES)
    });
    HINT_LINE * (1 + lines) as f32
}

/// Whether the line about the connection is shown: only while live.
pub(super) fn shows_health(m: &Model) -> bool {
    m.stream.state == StreamState::Live
}

/// What the connection is like, in a sentence, and how bad it is.
pub(super) fn health_sentence(m: &Model) -> (String, StreamHealth) {
    let (s, lang) = (&m.stream, m.lang);
    let verdict = match s.health {
        StreamHealth::Good => Key::StreamHealthGood,
        StreamHealth::Weak => Key::StreamHealthWeak,
        StreamHealth::Poor => Key::StreamHealthPoor,
    };
    let mut text = tr(lang, verdict).to_owned();
    if s.reconnects > 0 {
        text.push_str(" · ");
        text.push_str(&tr(lang, Key::StreamReconnects).replace("{n}", &s.reconnects.to_string()));
    }
    (text, s.health)
}

/// What the stream is doing, in a sentence.
pub(super) fn status_sentence(m: &Model) -> String {
    let (s, lang) = (&m.stream, m.lang);
    match s.state {
        StreamState::Offline | StreamState::Failed => tr(lang, Key::NavOffline).to_owned(),
        StreamState::Connecting => tr(lang, Key::NavConnecting).to_owned(),
        StreamState::Reconnecting => tr(lang, Key::NavReconnecting).to_owned(),
        StreamState::Live => format!(
            "{} · {}",
            tr(lang, Key::NavLive),
            tr(lang, Key::StreamLiveDetails)
                .replace("{kbps}", &s.bitrate_kbps.to_string())
                .replace("{lost}", &s.dropped_frames.to_string())
        ),
    }
}
