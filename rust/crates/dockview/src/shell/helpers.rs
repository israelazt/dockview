//! Small functions of the window side: what Windows says about animations and
//! the theme, how the video is filtered, and the helper thread.

use std::thread;

use dockview_domain::settings::RenderPreset;
use dockview_domain::theme::Appearance;
use dockview_platform::render::Filter;
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

pub fn filter_for(preset: RenderPreset) -> Filter {
    match preset {
        RenderPreset::Performance => Filter::Nearest,
        RenderPreset::Balanced | RenderPreset::Quality => Filter::Linear,
    }
}

pub fn appearance_of(theme: Option<WindowTheme>) -> Appearance {
    match theme {
        Some(WindowTheme::Light) => Appearance::Light,
        _ => Appearance::Dark,
    }
}

/// Runs `work` on a helper thread. Media Foundation and WASAPI want an MTA
/// thread, while winit initialises the main thread as an STA.
pub fn on_helper_thread<T: Default + Send + 'static>(
    work: impl FnOnce() -> Option<T> + Send + 'static,
) -> T {
    thread::spawn(work)
        .join()
        .ok()
        .flatten()
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_performance_uses_nearest_filtering() {
        assert_eq!(filter_for(RenderPreset::Performance), Filter::Nearest);
        assert_eq!(filter_for(RenderPreset::Balanced), Filter::Linear);
        assert_eq!(filter_for(RenderPreset::Quality), Filter::Linear);
    }
}
