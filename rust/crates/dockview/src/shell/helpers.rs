//! Small functions of the window side: what the system says about the theme and
//! how the video is filtered.

use dockview_domain::render_plan::render_plan;
pub use dockview_domain::render_plan::ScaleFilter;
use dockview_domain::settings::RenderPreset;
use dockview_domain::theme::Appearance;
use winit::window::Theme as WindowTheme;

/// How the video is scaled for a preset.
pub fn filter_for(preset: RenderPreset) -> ScaleFilter {
    render_plan(preset).filter
}

pub fn appearance_of(theme: Option<WindowTheme>) -> Appearance {
    match theme {
        Some(WindowTheme::Light) => Appearance::Light,
        _ => Appearance::Dark,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_preset_scales_the_video_its_own_way() {
        assert_eq!(filter_for(RenderPreset::Performance), ScaleFilter::Nearest);
        assert_eq!(filter_for(RenderPreset::Balanced), ScaleFilter::Linear);
        assert_eq!(filter_for(RenderPreset::Quality), ScaleFilter::Bicubic);
    }
}
