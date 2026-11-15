//! What the cards of the readouts say: the pieces of each one, from what the
//! app measures. No drawing here (see `hud.rs`).

use dockview_domain::hud::{format_of, latency_tier, LatencyTier};
use dockview_domain::i18n::{tr, Key};
use dockview_domain::stats::Stats;
use dockview_domain::theme::{Color, Theme};
use dockview_domain::view::{CurrentSource, Model, ReplayState, StatusKind};

/// One piece of a card, left to right.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Piece {
    Dot(Color),
    /// Two lines: a value on top and a small label under it.
    Stack(String, String, Color),
    /// A single line, centred.
    Line(String, Color),
    Divider,
    /// The small arrow that says the card opens a list.
    Chevron,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Card {
    pub(super) pieces: Vec<Piece>,
}

/// The name of what is being shown.
pub(super) fn source_name(m: &Model) -> String {
    let pick = |list: &[String], i: Option<usize>| i.and_then(|i| list.get(i).cloned());
    match m.sources.current {
        CurrentSource::Capture => pick(&m.video_devices, m.video_selected),
        CurrentSource::Monitor(i) => pick(&m.sources.monitors, Some(i)),
        CurrentSource::Window(i) => pick(&m.sources.windows, i),
    }
    .unwrap_or_else(|| "—".into())
}

pub(super) fn status_color(t: &Theme, status: StatusKind) -> Color {
    match status {
        StatusKind::Live => t.brand,
        StatusKind::Connecting => t.warn,
        StatusKind::Problem => t.error,
        StatusKind::Idle => t.text_dim,
    }
}

pub(super) fn tier_color(t: &Theme, tier: LatencyTier) -> Color {
    match tier {
        LatencyTier::UltraLow | LatencyTier::Low => t.brand,
        LatencyTier::Medium => t.warn,
        LatencyTier::High => t.error,
    }
}

/// The cards in the order they are given room (the last ones go first).
pub(super) fn cards(m: &Model, stats: &Stats) -> Vec<Card> {
    let t = &m.theme;
    let lang = m.lang;
    let mut out = Vec::new();

    // Where the picture comes from.
    let mut detail = m.resolution.clone();
    if let Some(format) = stats.device_mode.as_deref().and_then(format_of) {
        if !detail.is_empty() {
            detail.push_str(" · ");
        }
        detail.push_str(format);
    }
    out.push(Card {
        pieces: vec![
            Piece::Dot(status_color(t, m.status)),
            Piece::Stack(source_name(m), detail, t.text),
            Piece::Chevron,
        ],
    });

    // How long it takes.
    let (value, color, tag) = match stats.latency_ms.filter(|_| stats.measured) {
        Some(ms) => {
            let tier = latency_tier(ms);
            (
                format!("{ms:.1} ms"),
                tier_color(t, tier),
                Some(tr(lang, tier.label()).to_string()),
            )
        }
        None => ("…".to_string(), t.text_dim, None),
    };
    let mut latency = vec![Piece::Stack(
        value,
        tr(lang, Key::HudLatency).to_string(),
        t.text,
    )];
    if let Some(tag) = tag {
        latency.push(Piece::Line(tag, color));
    }
    out.push(Card { pieces: latency });

    // What it costs.
    let measured = stats.measured;
    let or_dots = |s: String| if measured { s } else { "…".to_string() };
    let cpu = if stats.cpu_core_percent < 10.0 {
        format!("{:.1}%", stats.cpu_core_percent)
    } else {
        format!("{:.0}%", stats.cpu_core_percent)
    };
    let mut perf = vec![
        Piece::Stack(
            or_dots(format!("{:.0} fps", stats.shown_fps)),
            tr(lang, Key::Shown).to_string(),
            t.text,
        ),
        Piece::Divider,
        Piece::Stack(
            format!("{}", m.nav.drops),
            tr(lang, Key::HudDrops).to_string(),
            if m.nav.drops > 0 { t.warn } else { t.text },
        ),
        Piece::Divider,
        Piece::Stack(format!("{:.0} MB", stats.memory_mb), "RAM".into(), t.text),
        Piece::Divider,
        Piece::Stack(or_dots(cpu), "CPU".into(), t.text),
    ];
    if let Some(ms) = stats.gpu_ms {
        perf.push(Piece::Divider);
        perf.push(Piece::Stack(format!("{ms:.2} ms"), "GPU".into(), t.text));
    }
    out.push(Card { pieces: perf });

    // Whether the replay is armed.
    let (key, dot) = match m.replay.state {
        ReplayState::Recording => (Key::HudReplayArmed, t.brand),
        ReplayState::Starting => (Key::HudReplayStarting, t.warn),
        ReplayState::Off => (Key::HudReplayOff, t.text_dim),
        ReplayState::Unavailable => (Key::HudReplayNone, t.error),
    };
    let mut replay = vec![
        Piece::Dot(dot),
        Piece::Stack(
            tr(lang, key).to_string(),
            format!("{}s · {}", m.replay.keep_seconds, m.replay.hotkey),
            t.text,
        ),
    ];
    if m.replay.hotkey.is_empty() {
        replay[1] = Piece::Line(tr(lang, key).to_string(), t.text);
    }
    out.push(Card { pieces: replay });
    out
}
