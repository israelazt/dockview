//! The Stream section: where to go live. The service (Twitch, YouTube, Kick or
//! another), the address of its server, the stream key, the frames per second
//! and the bitrate, the button that starts and stops the stream and what the
//! stream is doing. What is chosen goes through the same actions the rest of
//! the application uses; the key is typed here and never shown again.

use dockview_domain::broadcast::StreamState;
use dockview_domain::i18n::{tr, Key};
use dockview_domain::stream_health::StreamHealth;
use dockview_domain::theme::Theme;
use dockview_domain::view::{Action, Model};

use super::edit::EditTarget;
use super::recording::BUTTON_H;
use super::widgets::{GAP, LABEL_H, ROW_H};
use super::{intersect, PanelHit, StudioPanel, PAD};
use crate::fit::fit_text;
use crate::stats_data::wrap_text;
use crate::text::Weight;
use crate::{Rect, TextStyle, Ui};

const PAD_TOP: f32 = 4.0;
const PAD_BOTTOM: f32 = 14.0;
const STATUS_LINE: f32 = 18.0;
const HINT_LINE: f32 = 16.0;

const STATUS: TextStyle = TextStyle::new(13.0, Weight::SemiBold);
const FIELD: TextStyle = TextStyle::new(13.0, Weight::Regular);
const HINT: TextStyle = TextStyle::new(12.0, Weight::Regular);
const BUTTON_LABEL: TextStyle = TextStyle::new(13.5, Weight::SemiBold);

/// The services with a ready-made address, in the order they are offered.
/// (A service changes its servers now and then: the address stays editable.)
pub(super) const SERVICES: [(&str, &str); 3] = [
    ("Twitch", "rtmps://live.twitch.tv:443/app"),
    ("YouTube", "rtmps://a.rtmps.youtube.com:443/live2"),
    (
        "Kick",
        "rtmps://fa723fc1b171.global-contribute.live-video.net:443/app",
    ),
];
/// The choices of frames per second and of bitrate the section offers.
pub(super) const FPS_CHOICES: [u32; 2] = [30, 60];
pub(super) const KBPS_CHOICES: [u32; 3] = [3000, 4500, 6000];

/// Which service an address belongs to: the position in [`SERVICES`], or one
/// past the end for any other.
pub(super) fn service_of(address: &str) -> usize {
    let a = address.to_ascii_lowercase();
    if a.contains("twitch") {
        0
    } else if a.contains("youtube") {
        1
    } else if a.contains("kick") || a.contains("live-video.net") {
        2
    } else {
        SERVICES.len()
    }
}

/// The longest line of the hint the body leaves room for.
const HINT_LINES: f32 = 3.0;

/// How tall the body is: the status line, five groups of a caption and a row,
/// the button and the hint.
pub fn body_height(_ui: &mut Ui, m: &Model, _width: f32) -> f32 {
    PAD_TOP
        + STATUS_LINE
        + if shows_health(m) { HINT_LINE } else { 0.0 }
        + GAP
        + 5.0 * (LABEL_H + ROW_H + GAP)
        + BUTTON_H
        + 10.0
        + HINT_LINES * HINT_LINE
        + PAD_BOTTOM
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
fn status_sentence(m: &Model) -> String {
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

impl StudioPanel {
    /// Draws the body of the Stream section with its top-left corner at the
    /// corner of `body`.
    pub(super) fn draw_stream(&mut self, ui: &mut Ui, m: &Model, body: Rect) {
        let (t, lang, s) = (&m.theme, m.lang, &m.stream);
        let (x, w) = (body.x + PAD, body.w - PAD * 2.0);
        let mut y = body.y + PAD_TOP;
        let going = s.state != StreamState::Offline && s.state != StreamState::Failed;

        // What it is doing.
        let dot = if s.state == StreamState::Live {
            t.error
        } else if going {
            t.warn
        } else {
            t.text_dim
        };
        ui.rect(Rect::new(x, y + 6.0, 8.0, 8.0), 4.0, dot);
        let sentence = fit_text(ui, &status_sentence(m), STATUS, w - 18.0);
        ui.text(&sentence, x + 16.0, y, STATUS, t.text);
        y += STATUS_LINE;
        if shows_health(m) {
            let (text, health) = health_sentence(m);
            let color = match health {
                StreamHealth::Good => t.text_dim,
                StreamHealth::Weak => t.warn,
                StreamHealth::Poor => t.error,
            };
            let line = fit_text(ui, &text, HINT, w - 16.0);
            ui.text(&line, x + 16.0, y, HINT, color);
            y += HINT_LINE;
        }
        y += GAP;

        // The service.
        y += self.caption(ui, t, tr(lang, Key::StreamService), x, y);
        let labels: Vec<&str> = SERVICES
            .iter()
            .map(|(name, _)| *name)
            .chain([tr(lang, Key::StreamOther)])
            .collect();
        let actions: Vec<Action> = SERVICES
            .iter()
            .map(|(_, url)| Action::SetStreamUrl((*url).to_owned()))
            .chain([Action::SetStreamUrl(String::new())])
            .collect();
        y += self.segmented(
            ui,
            t,
            "stream-service",
            &labels,
            service_of(&s.address),
            actions,
            x,
            y,
            w,
        ) + GAP;

        // The address and the key.
        self.stream_url_of = s.address.clone();
        y += self.caption(ui, t, tr(lang, Key::StreamServer), x, y);
        let shown = (!s.address.is_empty()).then_some(s.address.as_str());
        y += self.text_field(
            ui,
            t,
            EditTarget::StreamUrl,
            (shown, tr(lang, Key::StreamServerHint)),
            (x, y, w),
        ) + GAP;
        y += self.caption(ui, t, tr(lang, Key::StreamKeyTitle), x, y);
        let (saved, hint) = if s.has_key {
            (Some("••••••••••••"), tr(lang, Key::StreamKeySaved))
        } else {
            (None, tr(lang, Key::StreamKeyHint))
        };
        y += self.text_field(ui, t, EditTarget::StreamKey, (saved, hint), (x, y, w)) + GAP;

        // The quality.
        y += self.caption(ui, t, tr(lang, Key::StreamFps), x, y);
        let fps_labels: Vec<String> = FPS_CHOICES.iter().map(|f| f.to_string()).collect();
        let fps_labels: Vec<&str> = fps_labels.iter().map(String::as_str).collect();
        y += self.segmented(
            ui,
            t,
            "stream-fps",
            &fps_labels,
            FPS_CHOICES.iter().position(|f| *f == s.fps).unwrap_or(0),
            FPS_CHOICES
                .iter()
                .map(|f| Action::SetStreamFps(*f))
                .collect(),
            x,
            y,
            w,
        ) + GAP;
        y += self.caption(ui, t, tr(lang, Key::StreamBitrate), x, y);
        let kbps_labels: Vec<String> = KBPS_CHOICES.iter().map(|k| k.to_string()).collect();
        let kbps_labels: Vec<&str> = kbps_labels.iter().map(String::as_str).collect();
        y += self.segmented(
            ui,
            t,
            "stream-kbps",
            &kbps_labels,
            KBPS_CHOICES
                .iter()
                .position(|k| *k == s.video_kbps)
                .unwrap_or(1),
            KBPS_CHOICES
                .iter()
                .map(|k| Action::SetStreamKbps(*k))
                .collect(),
            x,
            y,
            w,
        ) + GAP;

        // Go live or stop.
        self.live_button(
            ui,
            t,
            if going {
                StreamKind::Stop
            } else {
                StreamKind::Go
            },
            lang,
            (x, y, w),
        );
        y += BUTTON_H + 10.0;
        for line in wrap_text(ui, tr(lang, Key::StreamHint), HINT, w)
            .iter()
            .take(HINT_LINES as usize)
        {
            ui.text(line, x, y, HINT, t.text_dim);
            y += HINT_LINE;
        }
    }

    /// A line the user can click to type in. Shows `value` (or the `hint`
    /// when there is none); while it is being typed it shows the text with the
    /// caret, and the end of it when it is longer than the field. A key is
    /// shown as dots. Returns its height.
    fn text_field(
        &mut self,
        ui: &mut Ui,
        t: &Theme,
        target: EditTarget,
        (value, hint): (Option<&str>, &str),
        (x, y, w): (f32, f32, f32),
    ) -> f32 {
        let hit = if target == EditTarget::StreamKey {
            PanelHit::StreamKey
        } else {
            PanelHit::StreamUrl
        };
        let field = Rect::new(x, y, w, ROW_H);
        if let Some(shown) = intersect(field, self.clip) {
            self.register(hit, shown);
        }
        let typing = self
            .editing
            .as_ref()
            .filter(|(editing, _)| *editing == target)
            .map(|(_, e)| (e.text().to_owned(), e.before_caret().to_owned()));
        let hover = self.hover_amount(hit);
        let border = if typing.is_some() { t.accent } else { t.border };
        ui.panel(field, 9.0, t.surface_solid, border, 1.0);
        if typing.is_none() && hover > 0.002 {
            ui.rect(field, 9.0, t.hover.with_alpha(t.hover.a * hover));
        }
        let line_h = ui.measure("Ag", FIELD).1;
        let (text_x, room) = (x + 10.0, w - 20.0);
        let ty = y + (ROW_H - line_h) / 2.0;
        match typing {
            Some((typed, before_caret)) => {
                let mask = |s: &str| {
                    if target == EditTarget::StreamKey {
                        "•".repeat(s.chars().count())
                    } else {
                        s.to_owned()
                    }
                };
                let (typed, before) = (mask(&typed), mask(&before_caret));
                // The end of what was typed is what stays in view.
                let shown = tail_that_fits(ui, &typed, room - 4.0);
                ui.text(&shown, text_x, ty, FIELD, t.text);
                let hidden = typed.chars().count() - shown.chars().count();
                let before_visible: String = before.chars().skip(hidden).collect();
                let caret_x = text_x + ui.measure(&before_visible, FIELD).0;
                ui.rect(
                    Rect::new(caret_x, y + 8.0, 1.5, ROW_H - 16.0),
                    0.75,
                    t.accent,
                );
            }
            None => match value {
                Some(v) => {
                    let line = fit_text(ui, v, FIELD, room);
                    ui.text(&line, text_x, ty, FIELD, t.text);
                }
                None => {
                    let line = fit_text(ui, hint, FIELD, room);
                    ui.text(&line, text_x, ty, FIELD, t.text_dim);
                }
            },
        }
        ROW_H
    }

    /// The button that goes live or stops the stream.
    fn live_button(
        &mut self,
        ui: &mut Ui,
        t: &Theme,
        kind: StreamKind,
        lang: dockview_domain::i18n::Lang,
        (x, y, w): (f32, f32, f32),
    ) {
        let rect = Rect::new(x, y, w, BUTTON_H);
        let hit = self.item(rect, Action::ToggleStream);
        let hover = self.hover_amount(hit);
        let (fill, ink, label) = match kind {
            StreamKind::Go => (
                t.accent,
                dockview_domain::theme::Color::hex(0xFFFFFF, 1.0),
                tr(lang, Key::StreamGoLive),
            ),
            StreamKind::Stop => (
                t.error.with_alpha(0.9),
                dockview_domain::theme::Color::hex(0xFFFFFF, 1.0),
                tr(lang, Key::StreamStop),
            ),
        };
        ui.rect(rect, 12.0, fill);
        if hover > 0.002 {
            ui.rect(
                rect,
                12.0,
                dockview_domain::theme::Color::hex(0xFFFFFF, 0.14 * hover),
            );
        }
        let (label_w, label_h) = ui.measure(label, BUTTON_LABEL);
        ui.text(
            label,
            x + (w - label_w) / 2.0,
            y + (BUTTON_H - label_h) / 2.0,
            BUTTON_LABEL,
            ink,
        );
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StreamKind {
    Go,
    Stop,
}

/// The end of `text` that fits in `max_w` (all of it when it all fits).
fn tail_that_fits(ui: &mut Ui, text: &str, max_w: f32) -> String {
    if ui.measure(text, FIELD).0 <= max_w {
        return text.to_owned();
    }
    let chars: Vec<char> = text.chars().collect();
    for skip in 1..chars.len() {
        let candidate: String = chars[skip..].iter().collect();
        if ui.measure(&candidate, FIELD).0 <= max_w {
            return candidate;
        }
    }
    String::new()
}
