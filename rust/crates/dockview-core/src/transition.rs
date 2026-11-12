//! The change from one scene to another: layers that both scenes have glide
//! to their new place; the others fade in or out. Plain data and arithmetic,
//! no Windows; the app asks for the layers at time `t` and hands them to the
//! compositor.
//!
//! Layers are matched by their source, not by position, so a camera that sits
//! in one corner in scene A and another in scene B travels between them.
//! Only one *base* source runs at a time, so when the two bases differ the new
//! base is shown at once and the old one is not drawn.

use std::time::Duration;

use crate::scene::{Fit, NormRect, Scene, SceneLayer, SourceRef};

/// How long a scene change takes unless told otherwise.
pub const DEFAULT_DURATION: Duration = Duration::from_millis(400);

/// How the movement speeds up and slows down.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Easing {
    Linear,
    /// Slow start, slow end.
    #[default]
    EaseInOut,
    /// Slides a little past the target and settles back.
    Overshoot,
}

impl Easing {
    /// Maps linear progress `0..=1` to eased progress. Starts at 0 and ends
    /// at exactly 1; `Overshoot` goes beyond 1 on the way.
    pub fn apply(self, t: f32) -> f32 {
        let t = if t.is_finite() {
            t.clamp(0.0, 1.0)
        } else {
            1.0
        };
        match self {
            Self::Linear => t,
            Self::EaseInOut => t * t * (3.0 - 2.0 * t),
            Self::Overshoot => {
                // "Back out": the standard c1/c3 constants.
                const C1: f32 = 1.701_58;
                const C3: f32 = C1 + 1.0;
                let u = t - 1.0;
                1.0 + C3 * u * u * u + C1 * u * u
            }
        }
    }
}

/// A layer as the compositor should draw it at one moment.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawLayer {
    pub source: SourceRef,
    pub rect: NormRect,
    pub fit: Fit,
    /// 0 is invisible, 1 is fully drawn.
    pub opacity: f32,
}

/// One pairing between the two scenes.
#[derive(Debug, Clone)]
struct Track {
    source: SourceRef,
    from: Option<(NormRect, f32)>,
    to: Option<(NormRect, f32)>,
    fit: Fit,
}

/// A change between two scenes, ready to be sampled over time.
#[derive(Debug, Clone)]
pub struct Transition {
    tracks: Vec<Track>,
    duration: Duration,
    easing: Easing,
}

fn opacity_of(layer: &SceneLayer) -> f32 {
    if layer.visible {
        1.0
    } else {
        0.0
    }
}

impl Transition {
    /// Plans the change from `from` to `to`.
    pub fn new(from: &Scene, to: &Scene, duration: Duration, easing: Easing) -> Self {
        let same_base = from.base().is_some() && from.base() == to.base();
        let mut used = vec![false; from.layers.len()];
        let mut tracks = Vec::new();

        // The new scene's layers, bottom first.
        for (index, layer) in to.layers.iter().enumerate() {
            let old = from
                .layers
                .iter()
                .enumerate()
                .find(|(i, l)| !used[*i] && l.source == layer.source && (index > 0 || same_base))
                .map(|(i, l)| {
                    used[i] = true;
                    l
                });
            let to = (layer.rect, opacity_of(layer));
            tracks.push(Track {
                source: layer.source.clone(),
                // A new base has nothing to fade in over: it is there at once.
                from: match old {
                    Some(l) => Some((l.rect, opacity_of(l))),
                    None if index == 0 => Some(to),
                    None => None,
                },
                to: Some(to),
                fit: layer.fit,
            });
        }
        // Layers only the old scene has fade out, just above the base.
        let leaving = from.layers.iter().enumerate().filter_map(|(i, l)| {
            // An old base that is not the new base is gone at once.
            (!used[i] && (i > 0 || same_base)).then_some(Track {
                source: l.source.clone(),
                from: Some((l.rect, opacity_of(l))),
                to: None,
                fit: l.fit,
            })
        });
        let at = tracks.len().min(1);
        tracks.splice(at..at, leaving);

        Self {
            tracks,
            duration,
            easing,
        }
    }

    pub fn duration(&self) -> Duration {
        self.duration
    }

    /// Whether `elapsed` is past the end, so the target scene can simply be
    /// drawn as it is.
    pub fn is_done(&self, elapsed: Duration) -> bool {
        elapsed >= self.duration
    }

    /// The layers to draw `elapsed` after the start, bottom first. Layers
    /// that end up invisible are left out.
    pub fn sample(&self, elapsed: Duration) -> Vec<DrawLayer> {
        let linear = if self.duration.is_zero() {
            1.0
        } else {
            (elapsed.as_secs_f32() / self.duration.as_secs_f32()).clamp(0.0, 1.0)
        };
        let t = self.easing.apply(linear);
        // Opacity must stay within 0..=1 even when the movement overshoots.
        let fade = Easing::EaseInOut.apply(linear);
        self.tracks
            .iter()
            .filter_map(|track| {
                let (rect, opacity) = match (track.from, track.to) {
                    (Some((a, oa)), Some((b, ob))) => (lerp_rect(a, b, t), lerp(oa, ob, fade)),
                    (None, Some((b, ob))) => (b, ob * fade),
                    (Some((a, oa)), None) => (a, oa * (1.0 - fade)),
                    (None, None) => return None,
                };
                (opacity > 0.001).then(|| DrawLayer {
                    source: track.source.clone(),
                    rect: rect.clamped(),
                    fit: track.fit,
                    opacity: opacity.clamp(0.0, 1.0),
                })
            })
            .collect()
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    // Exact at both ends, so the last picture equals the target scene.
    if t >= 1.0 {
        b
    } else {
        a + (b - a) * t
    }
}

fn lerp_rect(a: NormRect, b: NormRect, t: f32) -> NormRect {
    NormRect::new(
        lerp(a.x, b.x, t),
        lerp(a.y, b.y, t),
        lerp(a.w, b.w, t),
        lerp(a.h, b.h, t),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(link: &str) -> SourceRef {
        SourceRef::Capture {
            symbolic_link: link.into(),
        }
    }

    const MS: fn(u64) -> Duration = Duration::from_millis;

    fn plan(from: &Scene, to: &Scene) -> Transition {
        Transition::new(from, to, MS(400), Easing::Linear)
    }

    #[test]
    fn easing_starts_at_zero_and_ends_at_one() {
        for easing in [Easing::Linear, Easing::EaseInOut, Easing::Overshoot] {
            assert!(easing.apply(0.0).abs() < 1e-6, "{easing:?}");
            assert!((easing.apply(1.0) - 1.0).abs() < 1e-6, "{easing:?}");
        }
        assert!((Easing::EaseInOut.apply(0.5) - 0.5).abs() < 1e-6);
        assert!(Easing::EaseInOut.apply(0.1) < 0.1);
        assert_eq!(Easing::Linear.apply(f32::NAN), 1.0);
    }

    #[test]
    fn overshoot_goes_past_the_target_and_comes_back() {
        let peak = (1..100)
            .map(|n| Easing::Overshoot.apply(n as f32 / 100.0))
            .fold(0.0_f32, f32::max);
        assert!(peak > 1.05 && peak < 1.2, "peak {peak}");
    }

    #[test]
    fn a_shared_layer_glides_between_its_two_places() {
        let mut a = Scene::with_base("A", card("game"));
        a.add_layer(card("cam")); // bottom right
        let mut b = Scene::with_base("B", card("game"));
        b.add_layer(card("cam"));
        b.layers[1].rect = NormRect::new(0.02, 0.02, 0.26, 0.26); // top left
        let t = plan(&a, &b);

        let start = t.sample(MS(0));
        let end = t.sample(MS(400));
        let mid = t.sample(MS(200));
        assert_eq!(start[1].rect, a.layers[1].rect.clamped());
        assert_eq!(end[1].rect, b.layers[1].rect.clamped());
        let (ra, rb) = (a.layers[1].rect, b.layers[1].rect);
        assert!((mid[1].rect.x - (ra.x + rb.x) / 2.0).abs() < 1e-5);
        assert_eq!(mid.len(), 2);
        assert!(mid.iter().all(|l| l.opacity == 1.0));
    }

    #[test]
    fn a_layer_only_the_new_scene_has_fades_in() {
        let a = Scene::with_base("A", card("game"));
        let mut b = Scene::with_base("B", card("game"));
        b.add_layer(card("cam"));
        let t = plan(&a, &b);

        assert_eq!(t.sample(MS(0)).len(), 1); // nothing yet
        let mid = t.sample(MS(200));
        assert_eq!(mid.len(), 2);
        assert!((mid[1].opacity - 0.5).abs() < 1e-5);
        assert_eq!(t.sample(MS(400))[1].opacity, 1.0);
    }

    #[test]
    fn a_layer_only_the_old_scene_has_fades_out_above_the_base() {
        let mut a = Scene::with_base("A", card("game"));
        a.add_layer(card("cam"));
        let mut b = Scene::with_base("B", card("game"));
        b.add_layer(card("mic-cam"));
        let t = plan(&a, &b);

        let mid = t.sample(MS(200));
        let sources: Vec<_> = mid.iter().map(|l| l.source.clone()).collect();
        assert_eq!(sources, vec![card("game"), card("cam"), card("mic-cam")]);
        assert!((mid[1].opacity - 0.5).abs() < 1e-5);
        // At the end the old layer is gone.
        let end = t.sample(MS(400));
        assert_eq!(end.len(), 2);
        assert_eq!(end[1].source, card("mic-cam"));
    }

    #[test]
    fn a_different_base_is_shown_at_once_and_the_old_one_is_dropped() {
        let a = Scene::with_base("A", card("game"));
        let b = Scene::with_base("B", card("console"));
        let t = plan(&a, &b);
        let start = t.sample(MS(0));
        assert_eq!(start.len(), 1);
        assert_eq!(start[0].source, card("console"));
        assert_eq!(start[0].opacity, 1.0);
    }

    #[test]
    fn hiding_and_showing_a_layer_fades_it() {
        let mut a = Scene::with_base("A", card("game"));
        a.add_layer(card("cam"));
        let mut b = a.clone();
        b.layers[1].visible = false;
        let t = plan(&a, &b);
        assert!((t.sample(MS(200))[1].opacity - 0.5).abs() < 1e-5);
        assert_eq!(t.sample(MS(400)).len(), 1);
        // And back.
        let t = plan(&b, &a);
        assert_eq!(t.sample(MS(0)).len(), 1);
        assert_eq!(t.sample(MS(400))[1].opacity, 1.0);
    }

    #[test]
    fn two_layers_with_the_same_source_are_paired_in_order() {
        let mut a = Scene::with_base("A", card("game"));
        a.add_layer(card("cam"));
        a.add_layer(card("cam"));
        let b = a.clone();
        let t = plan(&a, &b);
        // Nothing moves and nothing fades.
        let mid = t.sample(MS(123));
        assert_eq!(mid.len(), 3);
        assert!(mid.iter().all(|l| l.opacity == 1.0));
        assert_eq!(mid[2].rect, a.layers[2].rect.clamped());
    }

    #[test]
    fn overshooting_motion_stays_inside_the_picture_and_opacity_in_range() {
        let mut a = Scene::with_base("A", card("game"));
        a.add_layer(card("cam"));
        let mut b = a.clone();
        b.layers[1].rect = NormRect::new(0.0, 0.0, 0.3, 0.3);
        let t = Transition::new(&a, &b, MS(400), Easing::Overshoot);
        for ms in (0..=400).step_by(10) {
            for l in t.sample(MS(ms)) {
                assert!(l.rect.x >= 0.0 && l.rect.y >= 0.0);
                assert!(l.rect.x + l.rect.w <= 1.0 + 1e-5);
                assert!((0.0..=1.0).contains(&l.opacity));
            }
        }
    }

    #[test]
    fn a_zero_duration_is_already_done() {
        let a = Scene::with_base("A", card("game"));
        let t = Transition::new(&a, &a, Duration::ZERO, Easing::EaseInOut);
        assert!(t.is_done(Duration::ZERO));
        assert_eq!(t.sample(Duration::ZERO).len(), 1);
    }
}
