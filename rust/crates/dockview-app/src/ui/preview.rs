//! Static mock of the control pill, used by `--ui-preview` to judge the look
//! (typography, spacing, themes, languages) before the controls are live.

use super::text::Weight;
use super::{Rect, TextStyle, Ui};
use crate::i18n::{tr, Key, Lang};
use crate::theme::Theme;

const PILL_HEIGHT: f32 = 56.0;
const BOTTOM_MARGIN: f32 = 28.0;
const PAD_X: f32 = 22.0;
const GAP: f32 = 18.0;
const BUTTON: f32 = 32.0;
const ICON_SIZE: f32 = 17.0;
const SLIDER_WIDTH: f32 = 96.0;

// Lucide code points.
const ICON_VOLUME: char = '\u{e1ab}';
const ICON_MINUS: char = '\u{e11c}';
const ICON_PLUS: char = '\u{e13d}';
const ICON_SETTINGS: char = '\u{e29a}';
const ICON_FULLSCREEN: char = '\u{e112}';

const LABEL: TextStyle = TextStyle::new(13.0, Weight::Medium);
const BODY: TextStyle = TextStyle::new(13.0, Weight::Regular);

/// Sample values shown in the mock.
pub struct Sample {
    pub resolution: &'static str,
    pub volume: f32,
    pub sync_ms: i32,
    /// Index of the button drawn in its hover state (0 = settings).
    pub hovered_button: Option<usize>,
}

impl Default for Sample {
    fn default() -> Self {
        Self {
            resolution: "1080p · 60 fps",
            volume: 0.8,
            sync_ms: 10,
            hovered_button: Some(0),
        }
    }
}

/// Draws the whole preview scene on top of whatever is behind it.
pub fn draw(ui: &mut Ui, theme: &Theme, lang: Lang, sample: &Sample) {
    draw_pill(ui, theme, lang, sample);
}

fn draw_pill(ui: &mut Ui, theme: &Theme, lang: Lang, sample: &Sample) {
    let dim = BODY;
    let value = BODY.tabular();

    let live = tr(lang, Key::Live);
    let sync_label = tr(lang, Key::Sync);
    let percent = format!("{:.0}%", sample.volume * 100.0);
    let sync_value = format!("{:+} ms", sample.sync_ms).replace('-', "−");

    // Widths are measured with the widest plausible value so the pill does not
    // resize while numbers change.
    let live_w = ui.measure(live, LABEL).0;
    let res_w = ui.measure(sample.resolution, value).0;
    let sync_label_w = ui.measure(sync_label, dim).0;
    let percent_w = ui.measure("100%", value).0;
    let sync_value_w = ui.measure("−300 ms", value).0;

    let status_w = 8.0 + 8.0 + live_w + 12.0 + res_w;
    let volume_w = ICON_SIZE + 10.0 + SLIDER_WIDTH + 10.0 + percent_w;
    let sync_w = sync_label_w + 10.0 + BUTTON + sync_value_w + BUTTON;
    let buttons_w = BUTTON * 2.0 + 4.0;
    let divider = GAP * 2.0 + 1.0;
    let width = PAD_X * 2.0 + status_w + volume_w + sync_w + buttons_w + divider * 3.0;

    let (screen_w, screen_h) = ui.logical_size();
    let pill = Rect::new(
        ((screen_w - width) / 2.0).round(),
        (screen_h - BOTTOM_MARGIN - PILL_HEIGHT).round(),
        width.round(),
        PILL_HEIGHT,
    );
    let cy = pill.y + PILL_HEIGHT / 2.0;
    let radius = PILL_HEIGHT / 2.0;

    ui.shadow(pill, radius, 22.0, theme.shadow);
    ui.panel(pill, radius, theme.surface, theme.border, 1.0);

    let line_h = ui.measure("Ag", BODY).1;
    let text_y = cy - line_h / 2.0;
    let mut x = pill.x + PAD_X;

    // Status.
    ui.rect(Rect::new(x, cy - 4.0, 8.0, 8.0), 4.0, theme.live);
    x += 16.0;
    ui.text(live, x, text_y, LABEL, theme.text);
    x += live_w + 12.0;
    ui.text(sample.resolution, x, text_y, value, theme.text_dim);
    x += res_w;
    x = divider_at(ui, theme, x, cy);

    // Volume.
    ui.icon(ICON_VOLUME, x + ICON_SIZE / 2.0, cy, ICON_SIZE, theme.text);
    x += ICON_SIZE + 10.0;
    slider(
        ui,
        theme,
        Rect::new(x, cy - 6.0, SLIDER_WIDTH, 12.0),
        sample.volume,
    );
    x += SLIDER_WIDTH + 10.0;
    ui.text(&percent, x, text_y, value, theme.text_dim);
    x += percent_w;
    x = divider_at(ui, theme, x, cy);

    // Sync.
    ui.text(sync_label, x, text_y, dim, theme.text_dim);
    x += sync_label_w + 10.0;
    icon_button(ui, theme, ICON_MINUS, x + BUTTON / 2.0, cy, false, 14.0);
    x += BUTTON;
    let value_x = x + (sync_value_w - ui.measure(&sync_value, value).0) / 2.0;
    ui.text(&sync_value, value_x, text_y, value, theme.text);
    x += sync_value_w;
    icon_button(ui, theme, ICON_PLUS, x + BUTTON / 2.0, cy, false, 14.0);
    x += BUTTON;
    x = divider_at(ui, theme, x, cy);

    // Actions.
    for (i, icon) in [ICON_SETTINGS, ICON_FULLSCREEN].into_iter().enumerate() {
        let hovered = sample.hovered_button == Some(i);
        icon_button(ui, theme, icon, x + BUTTON / 2.0, cy, hovered, ICON_SIZE);
        x += BUTTON + if i == 0 { 4.0 } else { 0.0 };
    }
}

/// Draws a thin vertical divider after `x` and returns the next content x.
fn divider_at(ui: &mut Ui, theme: &Theme, x: f32, cy: f32) -> f32 {
    ui.rect(Rect::new(x + GAP, cy - 10.0, 1.0, 20.0), 0.5, theme.track);
    x + GAP * 2.0 + 1.0
}

fn icon_button(ui: &mut Ui, theme: &Theme, icon: char, cx: f32, cy: f32, hovered: bool, size: f32) {
    if hovered {
        ui.rect(
            Rect::new(cx - BUTTON / 2.0, cy - BUTTON / 2.0, BUTTON, BUTTON),
            BUTTON / 2.0,
            theme.hover,
        );
    }
    ui.icon(icon, cx, cy, size, theme.text);
}

fn slider(ui: &mut Ui, theme: &Theme, bounds: Rect, value: f32) {
    let (_, cy) = bounds.center();
    let track = Rect::new(bounds.x, cy - 2.0, bounds.w, 4.0);
    ui.rect(track, 2.0, theme.track);
    let fill = Rect::new(track.x, track.y, track.w * value.clamp(0.0, 1.0), track.h);
    ui.rect(fill, 2.0, theme.accent);
    let knob_x = track.x + track.w * value.clamp(0.0, 1.0);
    ui.rect(
        Rect::new(knob_x - 6.0, cy - 6.0, 12.0, 12.0),
        6.0,
        theme.text,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pill_bounds(lang: Lang, viewport: (u32, u32), scale: f32) -> (f32, f32) {
        let mut ui = Ui::new();
        ui.begin(viewport, scale);
        draw(&mut ui, &Theme::dark(), lang, &Sample::default());
        // The first quad is the shadow; the second is the pill's border rect.
        let v = ui.vertices();
        let xs: Vec<f32> = v.iter().map(|v| v.pos[0]).collect();
        (
            xs.iter().cloned().fold(f32::MAX, f32::min),
            xs.iter().cloned().fold(f32::MIN, f32::max),
        )
    }

    #[test]
    fn pill_is_centred_in_the_window() {
        let (min, max) = pill_bounds(Lang::En, (1280, 720), 1.0);
        let centre = (min + max) / 2.0;
        assert!((centre - 640.0).abs() < 2.0, "centre {centre}");
    }

    #[test]
    fn pill_fits_a_small_window_in_both_languages() {
        for lang in [Lang::En, Lang::Es] {
            let (min, max) = pill_bounds(lang, (1280, 720), 1.0);
            // The shadow extends past the pill, so leave it room.
            assert!(min >= 0.0 && max <= 1280.0, "{lang:?}: {min}..{max}");
        }
    }

    #[test]
    fn spanish_pill_is_wider_than_english() {
        let en = pill_bounds(Lang::En, (1280, 720), 1.0);
        let es = pill_bounds(Lang::Es, (1280, 720), 1.0);
        assert!(es.1 - es.0 > en.1 - en.0);
    }

    #[test]
    fn higher_dpi_scales_the_pill_proportionally() {
        let one = pill_bounds(Lang::En, (1280, 720), 1.0);
        let two = pill_bounds(Lang::En, (2560, 1440), 2.0);
        let ratio = (two.1 - two.0) / (one.1 - one.0);
        assert!((ratio - 2.0).abs() < 0.1, "ratio {ratio}");
    }
}
