//! Scenes: which sources are drawn, and where. Plain data, saved in the
//! settings; the compositor ([`crate::compose`]) turns it into a picture.
//!
//! A scene is a list of layers, bottom first. The first layer is the *base*:
//! it is the main source, the one whose sound is recorded. The others are
//! drawn over it.

use serde::{Deserialize, Serialize};

/// Most layers one scene may have.
pub const MAX_SCENE_LAYERS: usize = 8;
/// Smallest width or height of a layer, as a fraction of the picture.
pub const MIN_LAYER_SIZE: f32 = 0.05;

/// A rectangle as fractions of the output picture: `0.0, 0.0, 1.0, 1.0` is all
/// of it. Free-form, so the editor can place layers anywhere.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NormRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl NormRect {
    pub const FULL: Self = Self {
        x: 0.0,
        y: 0.0,
        w: 1.0,
        h: 1.0,
    };

    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    /// The same rectangle pulled back inside the picture, with a sensible
    /// minimum size. Also repairs NaN and infinite values from a damaged file.
    #[must_use]
    pub fn clamped(self) -> Self {
        let fix = |v: f32, fallback: f32| if v.is_finite() { v } else { fallback };
        let w = fix(self.w, 1.0).clamp(MIN_LAYER_SIZE, 1.0);
        let h = fix(self.h, 1.0).clamp(MIN_LAYER_SIZE, 1.0);
        Self {
            x: fix(self.x, 0.0).clamp(0.0, 1.0 - w),
            y: fix(self.y, 0.0).clamp(0.0, 1.0 - h),
            w,
            h,
        }
    }

    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x <= self.x + self.w && y >= self.y && y <= self.y + self.h
    }
}

/// How a source's picture fills its rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Fit {
    /// The whole picture, kept in proportion, with black bars if needed.
    #[default]
    Contain,
    /// The rectangle is filled in proportion, cropping what sticks out.
    Cover,
    /// The whole picture stretched to the rectangle.
    Stretch,
}

/// Where a layer's picture comes from, in a form that survives a restart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SourceRef {
    /// A capture card or camera, by the link Windows gives the device.
    #[serde(rename_all = "camelCase")]
    Capture { symbolic_link: String },
    /// A monitor, by its device name (for example `\\.\DISPLAY2`).
    #[serde(rename_all = "camelCase")]
    Monitor { device_name: String },
    /// A window, by its title.
    Window { title: String },
}

/// One layer of a scene.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneLayer {
    pub source: SourceRef,
    pub rect: NormRect,
    #[serde(default)]
    pub fit: Fit,
    #[serde(default = "yes")]
    pub visible: bool,
}

fn yes() -> bool {
    true
}

impl SceneLayer {
    pub fn new(source: SourceRef, rect: NormRect) -> Self {
        Self {
            source,
            rect,
            fit: Fit::Contain,
            visible: true,
        }
    }
}

/// A named arrangement of layers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scene {
    pub name: String,
    pub layers: Vec<SceneLayer>,
}

/// Width (and, as the picture is 16:9, height) of a newly added layer.
const NEW_LAYER_SIZE: f32 = 0.26;
/// Distance of a newly added layer from the edges.
const NEW_LAYER_MARGIN: f32 = 0.02;

impl Scene {
    /// A scene with just its base source, over the whole picture.
    pub fn with_base(name: impl Into<String>, base: SourceRef) -> Self {
        Self {
            name: name.into(),
            layers: vec![SceneLayer::new(base, NormRect::FULL)],
        }
    }

    /// The base layer's source: the main one.
    pub fn base(&self) -> Option<&SourceRef> {
        self.layers.first().map(|l| &l.source)
    }

    /// Adds a layer over the others, in the first free corner (bottom right
    /// first). Returns its position, or `None` when the scene is full.
    pub fn add_layer(&mut self, source: SourceRef) -> Option<usize> {
        if self.layers.len() >= MAX_SCENE_LAYERS {
            return None;
        }
        let size = NEW_LAYER_SIZE;
        let far = 1.0 - size - NEW_LAYER_MARGIN;
        let near = NEW_LAYER_MARGIN;
        let corners = [(far, far), (near, far), (far, near), (near, near)];
        let taken = |x: f32, y: f32| {
            self.layers
                .iter()
                .skip(1)
                .any(|l| (l.rect.x - x).abs() < 0.05 && (l.rect.y - y).abs() < 0.05)
        };
        let (x, y) = corners
            .into_iter()
            .find(|&(x, y)| !taken(x, y))
            .unwrap_or(corners[0]);
        self.layers
            .push(SceneLayer::new(source, NormRect::new(x, y, size, size)));
        Some(self.layers.len() - 1)
    }

    /// Fixes whatever a hand-edited or damaged file could break.
    #[must_use]
    pub fn sanitized(mut self) -> Self {
        self.layers.truncate(MAX_SCENE_LAYERS);
        for layer in &mut self.layers {
            layer.rect = layer.rect.clamped();
        }
        self.name = self.name.trim().chars().take(40).collect();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(link: &str) -> SourceRef {
        SourceRef::Capture {
            symbolic_link: link.into(),
        }
    }

    #[test]
    fn a_scene_survives_a_round_trip_through_json() {
        let mut scene = Scene::with_base("Juego + cámara", card("usb#a"));
        scene.add_layer(SourceRef::Window {
            title: "Mi juego".into(),
        });
        scene.add_layer(SourceRef::Monitor {
            device_name: r"\\.\DISPLAY2".into(),
        });
        scene.layers[1].fit = Fit::Cover;
        scene.layers[2].visible = false;
        let json = serde_json::to_string(&scene).unwrap();
        assert_eq!(serde_json::from_str::<Scene>(&json).unwrap(), scene);
    }

    #[test]
    fn the_file_format_is_readable_and_forgiving() {
        let json = r#"{"name":"A","layers":[
            {"source":{"kind":"capture","symbolicLink":"x"},
             "rect":{"x":0,"y":0,"w":1,"h":1}}]}"#;
        let scene: Scene = serde_json::from_str(json).unwrap();
        // fit and visible may be missing
        assert_eq!(scene.layers[0].fit, Fit::Contain);
        assert!(scene.layers[0].visible);
        assert_eq!(scene.base(), Some(&card("x")));
    }

    #[test]
    fn new_layers_take_the_corners_in_turn() {
        let mut scene = Scene::with_base("A", card("base"));
        for n in 0..4 {
            assert_eq!(scene.add_layer(card(&format!("c{n}"))), Some(n + 1));
        }
        let origins: Vec<(i32, i32)> = scene
            .layers
            .iter()
            .skip(1)
            .map(|l| {
                (
                    (l.rect.x * 100.0).round() as i32,
                    (l.rect.y * 100.0).round() as i32,
                )
            })
            .collect();
        assert_eq!(origins, vec![(72, 72), (2, 72), (72, 2), (2, 2)]);
        // Past the fourth corner they pile onto the first one.
        assert!(scene.add_layer(card("c4")).is_some());
    }

    #[test]
    fn a_scene_cannot_grow_without_limit() {
        let mut scene = Scene::with_base("A", card("base"));
        while scene.add_layer(card("x")).is_some() {}
        assert_eq!(scene.layers.len(), MAX_SCENE_LAYERS);
    }

    #[test]
    fn rectangles_are_pulled_back_inside_the_picture() {
        let r = NormRect::new(0.9, 0.9, 0.5, 0.5).clamped();
        assert!(r.x + r.w <= 1.0 + f32::EPSILON && r.y + r.h <= 1.0 + f32::EPSILON);
        let tiny = NormRect::new(0.0, 0.0, 0.0, -3.0).clamped();
        assert_eq!((tiny.w, tiny.h), (MIN_LAYER_SIZE, MIN_LAYER_SIZE));
        let broken = NormRect::new(f32::NAN, f32::INFINITY, f32::NAN, f32::NAN).clamped();
        assert!(broken.x.is_finite() && broken.y.is_finite());
        assert_eq!((broken.w, broken.h), (1.0, 1.0));
    }

    #[test]
    fn sanitizing_repairs_a_damaged_scene() {
        let mut scene = Scene::with_base(
            "  una escena con un nombre larguísimo que no cabe en la lista del popover  ",
            card("b"),
        );
        scene.layers[0].rect = NormRect::new(-5.0, 7.0, 9.0, 0.0);
        let scene = scene.sanitized();
        assert!(scene.name.chars().count() <= 40);
        assert!(!scene.name.starts_with(' '));
        assert_eq!(scene.layers[0].rect.w, 1.0);
        assert_eq!(scene.layers[0].rect.h, MIN_LAYER_SIZE);
    }

    #[test]
    fn hit_testing_uses_the_rectangle() {
        let r = NormRect::new(0.5, 0.5, 0.25, 0.25);
        assert!(r.contains(0.6, 0.6));
        assert!(!r.contains(0.4, 0.6));
        assert!(!r.contains(0.6, 0.8));
    }
}
