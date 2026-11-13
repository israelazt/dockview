//! Colour tokens for the dark and light themes.

/// Straight (non-premultiplied) RGBA colour, components in 0.0..=1.0.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    /// Builds a colour from a `0xRRGGBB` value and an alpha.
    pub const fn hex(rgb: u32, alpha: f32) -> Self {
        Self {
            r: ((rgb >> 16) & 0xFF) as f32 / 255.0,
            g: ((rgb >> 8) & 0xFF) as f32 / 255.0,
            b: (rgb & 0xFF) as f32 / 255.0,
            a: alpha,
        }
    }

    pub const fn with_alpha(self, a: f32) -> Self {
        Self { a, ..self }
    }

    pub fn to_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }

    /// WCAG relative luminance of the colour, ignoring alpha.
    pub fn luminance(self) -> f32 {
        fn channel(c: f32) -> f32 {
            if c <= 0.03928 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * channel(self.r) + 0.7152 * channel(self.g) + 0.0722 * channel(self.b)
    }
}

/// WCAG contrast ratio between two colours (1.0 – 21.0).
pub fn contrast_ratio(a: Color, b: Color) -> f32 {
    let (la, lb) = (a.luminance(), b.luminance());
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

pub use dockview_domain::settings::ThemeMode;

/// A resolved theme (never "auto").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Appearance {
    Dark,
    Light,
}

/// Resolving the stored preference against the system appearance.
pub trait ThemeModeExt {
    fn resolve(self, system: Appearance) -> Appearance;
}

impl ThemeModeExt for ThemeMode {
    /// Resolves the preference given the system appearance.
    fn resolve(self, system: Appearance) -> Appearance {
        match self {
            Self::Auto => system,
            Self::Dark => Appearance::Dark,
            Self::Light => Appearance::Light,
        }
    }
}

/// Semantic colours used by the interface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Theme {
    pub appearance: Appearance,
    /// Translucent panel behind controls (pill, popover).
    pub surface: Color,
    /// Opaque equivalent of `surface`, used for contrast checks.
    pub surface_solid: Color,
    pub border: Color,
    pub text: Color,
    pub text_dim: Color,
    pub accent: Color,
    pub live: Color,
    pub error: Color,
    /// Connecting / degraded states.
    pub warn: Color,
    /// Empty part of a slider.
    pub track: Color,
    /// Background of a hovered or pressed control.
    pub hover: Color,
    /// Background of a segmented control.
    pub segment_track: Color,
    /// The selected segment of a segmented control.
    pub segment_active: Color,
    pub shadow: Color,
}

impl Theme {
    pub fn for_appearance(appearance: Appearance) -> Self {
        match appearance {
            Appearance::Dark => Self::dark(),
            Appearance::Light => Self::light(),
        }
    }

    pub const fn dark() -> Self {
        Self {
            appearance: Appearance::Dark,
            surface: Color::hex(0x141414, 0.88),
            surface_solid: Color::hex(0x141414, 1.0),
            border: Color::hex(0xFFFFFF, 0.10),
            text: Color::hex(0xF2F2F2, 1.0),
            text_dim: Color::hex(0x9A9A9A, 1.0),
            accent: Color::hex(0x00AAFF, 1.0),
            live: Color::hex(0x22C55E, 1.0),
            error: Color::hex(0xEF4444, 1.0),
            warn: Color::hex(0xF59E0B, 1.0),
            track: Color::hex(0xFFFFFF, 0.18),
            hover: Color::hex(0xFFFFFF, 0.10),
            segment_track: Color::hex(0xFFFFFF, 0.07),
            segment_active: Color::hex(0xFFFFFF, 0.16),
            shadow: Color::hex(0x000000, 0.32),
        }
    }

    pub const fn light() -> Self {
        Self {
            appearance: Appearance::Light,
            surface: Color::hex(0xFFFFFF, 0.92),
            surface_solid: Color::hex(0xFFFFFF, 1.0),
            border: Color::hex(0x000000, 0.08),
            text: Color::hex(0x111111, 1.0),
            text_dim: Color::hex(0x5F5F5F, 1.0),
            accent: Color::hex(0x0A84D0, 1.0),
            live: Color::hex(0x16A34A, 1.0),
            error: Color::hex(0xDC2626, 1.0),
            warn: Color::hex(0xB45309, 1.0),
            track: Color::hex(0x000000, 0.14),
            hover: Color::hex(0x000000, 0.07),
            segment_track: Color::hex(0x000000, 0.06),
            segment_active: Color::hex(0xFFFFFF, 1.0),
            shadow: Color::hex(0x000000, 0.14),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_parses_channels() {
        let c = Color::hex(0xFF8000, 0.5);
        assert_eq!((c.r, c.a), (1.0, 0.5));
        assert!((c.g - 128.0 / 255.0).abs() < 1e-6);
        assert_eq!(c.b, 0.0);
    }

    #[test]
    fn contrast_of_black_on_white_is_21() {
        let ratio = contrast_ratio(Color::hex(0x000000, 1.0), Color::hex(0xFFFFFF, 1.0));
        assert!((ratio - 21.0).abs() < 0.01);
    }

    #[test]
    fn both_themes_meet_wcag_contrast() {
        for theme in [Theme::dark(), Theme::light()] {
            let bg = theme.surface_solid;
            // Body text: AAA. Secondary text: AA. Non-text accents/states: 3:1.
            assert!(
                contrast_ratio(theme.text, bg) >= 7.0,
                "{:?} text",
                theme.appearance
            );
            assert!(
                contrast_ratio(theme.text_dim, bg) >= 4.5,
                "{:?} dim",
                theme.appearance
            );
            assert!(
                contrast_ratio(theme.accent, bg) >= 3.0,
                "{:?} accent",
                theme.appearance
            );
            assert!(
                contrast_ratio(theme.live, bg) >= 3.0,
                "{:?} live",
                theme.appearance
            );
            assert!(
                contrast_ratio(theme.error, bg) >= 3.0,
                "{:?} error",
                theme.appearance
            );
            assert!(
                contrast_ratio(theme.warn, bg) >= 3.0,
                "{:?} warn",
                theme.appearance
            );
        }
    }

    #[test]
    fn auto_follows_the_system_and_manual_overrides_it() {
        assert_eq!(
            ThemeMode::Auto.resolve(Appearance::Light),
            Appearance::Light
        );
        assert_eq!(ThemeMode::Auto.resolve(Appearance::Dark), Appearance::Dark);
        assert_eq!(ThemeMode::Dark.resolve(Appearance::Light), Appearance::Dark);
        assert_eq!(
            ThemeMode::Light.resolve(Appearance::Dark),
            Appearance::Light
        );
    }
}
