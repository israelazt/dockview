//! Small functions of the window side: what Windows says about animations and
//! the theme, how the video is filtered, and the helper thread.

use dockview_domain::render_plan::render_plan;
pub use dockview_domain::render_plan::ScaleFilter;
use dockview_domain::settings::RenderPreset;
use dockview_domain::theme::Appearance;
use winit::window::Theme as WindowTheme;

/// Whether Windows allows UI animations ("Show animations in Windows").
pub fn system_animations_enabled() -> bool {
    use windows::core::BOOL;
    use windows::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPI_GETCLIENTAREAANIMATION, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    };
    let mut enabled = BOOL(1);
    // SAFETY: the out-pointer refers to a live BOOL, as SPI_GETCLIENTAREAANIMATION requires.
    let result = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some((&mut enabled as *mut BOOL).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    result.map_or(true, |()| enabled.as_bool())
}

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
