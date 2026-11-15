//! The status strip over the cards of the Studio: which scene is on air and
//! how the picture is doing (frames, delay, losses). It only shows. The data
//! is a plain function of what the app measures, tested without a window.

use dockview_domain::design::RADIUS_CARD;
use dockview_domain::hud::latency_tier;
use dockview_domain::i18n::{tr, Key};
use dockview_domain::stats::Stats;
use dockview_domain::theme::Color;
use dockview_domain::view::Model;

use super::hud_cards::{status_color, tier_color};
use super::text::Weight;
use super::{Rect, TextStyle, Ui};

/// Height of the strip, in logical pixels.
pub const STATUS_H: f32 = 36.0;
const PAD: f32 = 16.0;
const GAP: f32 = 18.0;
const NAME: TextStyle = TextStyle::new(13.0, Weight::SemiBold);
const VALUE: TextStyle = TextStyle::new(12.5, Weight::Medium);
const LABEL: TextStyle = TextStyle::new(11.5, Weight::Regular);

/// One figure of the strip: its value, the unit or name after it and its tone.
#[derive(Debug, Clone, PartialEq)]
pub struct Figure {
    pub value: String,
    pub label: String,
    pub color: Color,
}

/// What the strip says.
#[derive(Debug, Clone, PartialEq)]
pub struct StatusData {
    /// The scene in use (the plain source when there is none).
    pub scene: String,
    pub dot: Color,
    /// Most important first: the last ones go first when there is no room.
    pub figures: Vec<Figure>,
}

pub fn data(m: &Model, stats: &Stats) -> StatusData {
    let t = &m.theme;
    let scene = m
        .scenes
        .active
        .and_then(|i| m.scenes.names.get(i).cloned())
        .unwrap_or_else(|| tr(m.lang, Key::ScenePlain).to_string());
    let measured = stats.measured;
    let fps = if measured {
        format!("{:.0}", stats.shown_fps)
    } else {
        "…".to_string()
    };
    let (latency, latency_color) = match stats.latency_ms.filter(|_| measured) {
        Some(ms) => (format!("{ms:.1}"), tier_color(t, latency_tier(ms))),
        None => ("…".to_string(), t.text_dim),
    };
    let drops = m.nav.drops;
    StatusData {
        scene,
        dot: status_color(t, m.status),
        figures: vec![
            Figure {
                value: fps,
                label: "fps".into(),
                color: t.text,
            },
            Figure {
                value: latency,
                label: "ms".into(),
                color: latency_color,
            },
            Figure {
                value: drops.to_string(),
                label: tr(m.lang, Key::HudDrops).to_string(),
                color: if drops > 0 { t.warn } else { t.text },
            },
        ],
    }
}

/// Draws the strip as a card in `area`.
pub fn draw(ui: &mut Ui, m: &Model, stats: &Stats, area: Rect) {
    let t = &m.theme;
    let d = data(m, stats);
    ui.panel(area, RADIUS_CARD, t.surface_solid, t.border, 1.0);
    let cy = area.y + area.h / 2.0;
    let line = |ui: &mut Ui, style| ui.measure("Ag", style).1;
    ui.rect(
        Rect::new(area.x + PAD, cy - 3.5, 7.0, 7.0),
        3.5,
        d.dot,
    );
    let left = area.x + PAD + 7.0 + 8.0;
    // The scene name takes what the figures leave.
    let mut right = area.x + area.w - PAD;
    let mut shown = Vec::new();
    for fig in &d.figures {
        let w = ui.measure(&fig.value, VALUE).0 + 4.0 + ui.measure(&fig.label, LABEL).0;
        if right - w - GAP < left + 80.0 {
            break;
        }
        right -= w;
        shown.push((fig, right));
        right -= GAP;
    }
    let (vh, lh) = (line(ui, VALUE), line(ui, LABEL));
    for (fig, x) in shown {
        let vw = ui.text(&fig.value, x, cy - vh / 2.0, VALUE, fig.color);
        ui.text(&fig.label, x + vw + 4.0, cy - lh / 2.0, LABEL, t.text_dim);
    }
    let nh = line(ui, NAME);
    ui.set_clip(Some(Rect::new(left, area.y, (right - left).max(0.0), area.h)));
    ui.text(&d.scene, left, cy - nh / 2.0, NAME, t.text);
    ui.set_clip(None);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_model::model;

    fn measured() -> Stats {
        Stats {
            measured: true,
            shown_fps: 59.6,
            latency_ms: Some(12.34),
            ..Stats::default()
        }
    }

    #[test]
    fn it_names_the_scene_in_use_or_the_plain_source() {
        let mut m = model();
        assert_eq!(data(&m, &measured()).scene, tr(m.lang, Key::ScenePlain));
        m.scenes.names = vec!["Juego".into(), "Charla".into()];
        m.scenes.active = Some(1);
        assert_eq!(data(&m, &measured()).scene, "Charla");
        m.scenes.active = Some(7);
        assert_eq!(data(&m, &measured()).scene, tr(m.lang, Key::ScenePlain));
    }

    #[test]
    fn it_gives_frames_delay_and_losses_in_that_order() {
        let mut m = model();
        m.nav.drops = 3;
        let figures = data(&m, &measured()).figures;
        let shown: Vec<_> = figures
            .iter()
            .map(|f| (f.value.as_str(), f.label.as_str()))
            .collect();
        assert_eq!(shown[0], ("60", "fps"));
        assert_eq!(shown[1], ("12.3", "ms"));
        assert_eq!(shown[2].0, "3");
        assert_eq!(figures[2].color, m.theme.warn, "losses stand out");
    }

    #[test]
    fn before_the_first_measurement_it_shows_dots_and_not_zeros() {
        let m = model();
        let figures = data(&m, &Stats::default()).figures;
        assert_eq!(figures[0].value, "…");
        assert_eq!(figures[1].value, "…");
        assert_eq!(figures[1].color, m.theme.text_dim);
    }
}
