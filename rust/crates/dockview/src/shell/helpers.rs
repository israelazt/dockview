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

/// What to do with the window's maximized state when full screen is turned on
/// or off. A maximized window that goes to full screen keeps its client area
/// at the work area (the taskbar's room is left out) and stays broken after
/// leaving, so it is un-maximized first and maximized again afterwards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FullscreenPlan {
    /// Take the maximized state off before changing full screen.
    pub unmaximize_first: bool,
    /// Put the maximized state back after changing full screen.
    pub maximize_after: bool,
    /// Whether the window has to be maximized again when full screen ends.
    pub remember_maximized: bool,
}

/// `maximized`: the window is maximized now; `remembered`: it was before it
/// went to full screen.
pub fn fullscreen_plan(on: bool, maximized: bool, remembered: bool) -> FullscreenPlan {
    if on {
        FullscreenPlan {
            unmaximize_first: maximized,
            maximize_after: false,
            // Going to full screen again while already there changes nothing.
            remember_maximized: maximized || remembered,
        }
    } else {
        FullscreenPlan {
            unmaximize_first: false,
            maximize_after: remembered,
            remember_maximized: false,
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_maximized_window_is_un_maximized_before_full_screen_and_remembered() {
        let plan = fullscreen_plan(true, true, false);
        assert!(plan.unmaximize_first && plan.remember_maximized && !plan.maximize_after);
    }

    #[test]
    fn a_normal_window_goes_to_full_screen_untouched() {
        let plan = fullscreen_plan(true, false, false);
        assert!(!plan.unmaximize_first && !plan.remember_maximized && !plan.maximize_after);
    }

    #[test]
    fn leaving_full_screen_maximizes_again_only_if_it_was() {
        let was = fullscreen_plan(false, false, true);
        assert!(was.maximize_after && !was.remember_maximized);
        let was_not = fullscreen_plan(false, false, false);
        assert!(!was_not.maximize_after);
    }

    #[test]
    fn asking_for_full_screen_twice_does_not_forget_it_was_maximized() {
        // The second request finds the window already un-maximized.
        let again = fullscreen_plan(true, false, true);
        assert!(again.remember_maximized && !again.unmaximize_first);
    }

    use super::*;

    #[test]
    fn each_preset_scales_the_video_its_own_way() {
        assert_eq!(filter_for(RenderPreset::Performance), ScaleFilter::Nearest);
        assert_eq!(filter_for(RenderPreset::Balanced), ScaleFilter::Linear);
        assert_eq!(filter_for(RenderPreset::Quality), ScaleFilter::Bicubic);
    }
}
