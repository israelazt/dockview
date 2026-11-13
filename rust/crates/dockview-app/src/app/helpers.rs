//! Small functions the app uses: the render preset, list wrapping, and the
//! Windows settings and threads it asks about.

use super::*;

/// Whether Windows allows UI animations ("Show animations in Windows").
pub(super) fn system_animations_enabled() -> bool {
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

pub(super) fn filter_for(preset: RenderPreset) -> Filter {
    match preset {
        RenderPreset::Performance => Filter::Nearest,
        RenderPreset::Balanced | RenderPreset::Quality => Filter::Linear,
    }
}

pub(super) fn appearance_of(theme: Option<WindowTheme>) -> Appearance {
    match theme {
        Some(WindowTheme::Light) => Appearance::Light,
        _ => Appearance::Dark,
    }
}

pub(super) fn next_preset(preset: RenderPreset) -> RenderPreset {
    match preset {
        RenderPreset::Performance => RenderPreset::Balanced,
        RenderPreset::Balanced => RenderPreset::Quality,
        RenderPreset::Quality => RenderPreset::Performance,
    }
}

/// Index after `current` in a list of `len` items, wrapping around.
pub(super) fn next_index(current: Option<usize>, len: usize) -> Option<usize> {
    (len > 0).then(|| current.map_or(0, |i| (i + 1) % len))
}

/// Runs `work` on a helper thread. Media Foundation and WASAPI want an MTA
/// thread, while winit initialises the main thread as an STA.
pub(super) fn on_helper_thread<T: Default + Send + 'static>(
    work: impl FnOnce() -> Option<T> + Send + 'static,
) -> T {
    thread::spawn(work)
        .join()
        .ok()
        .flatten()
        .unwrap_or_default()
}

/// What the controls show next to the status light: `1080p · 50 fps`. The
/// rate is unknown for the first second, then only the height is shown.
pub(super) fn resolution_label(video_size: (u32, u32), shown_fps: f32) -> String {
    if video_size.0 == 0 {
        String::new()
    } else if shown_fps >= 1.0 {
        format!("{}p · {:.0} fps", video_size.1, shown_fps)
    } else {
        format!("{}p", video_size.1)
    }
}

/// The microphone the list entry `index` stands for: entry 0 is the Windows
/// default (no id), the others follow the audio devices.
pub(super) fn mic_device_id(index: usize, devices: &[AudioDevice]) -> Option<String> {
    index
        .checked_sub(1)
        .and_then(|i| devices.get(i))
        .map(|d| d.device_id.clone())
}

/// The list entry of the microphone with id `chosen` (0 when none, or it is
/// gone): the opposite of [`mic_device_id`].
pub(super) fn mic_index_of(chosen: Option<&str>, devices: &[AudioDevice]) -> usize {
    chosen
        .and_then(|id| devices.iter().position(|d| d.device_id == id))
        .map_or(0, |i| i + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_index_wraps_and_handles_empty_lists() {
        assert_eq!(next_index(None, 0), None);
        assert_eq!(next_index(None, 3), Some(0));
        assert_eq!(next_index(Some(0), 3), Some(1));
        assert_eq!(next_index(Some(2), 3), Some(0));
    }

    #[test]
    fn presets_cycle_through_all_three() {
        let start = RenderPreset::Performance;
        let cycled = next_preset(next_preset(next_preset(start)));
        assert_eq!(cycled, start);
        assert_ne!(next_preset(start), start);
    }

    #[test]
    fn only_performance_uses_nearest_filtering() {
        assert_eq!(filter_for(RenderPreset::Performance), Filter::Nearest);
        assert_eq!(filter_for(RenderPreset::Balanced), Filter::Linear);
        assert_eq!(filter_for(RenderPreset::Quality), Filter::Linear);
    }

    #[test]
    fn the_resolution_waits_for_a_size_and_then_for_a_rate() {
        assert_eq!(resolution_label((0, 0), 60.0), "");
        assert_eq!(resolution_label((1920, 1080), 0.0), "1080p");
        assert_eq!(resolution_label((1920, 1080), 0.9), "1080p");
        assert_eq!(resolution_label((1920, 1080), 49.9), "1080p · 50 fps");
    }

    #[test]
    fn microphone_entries_and_ids_map_both_ways() {
        let device = |id: &str| AudioDevice {
            friendly_name: id.to_uppercase(),
            device_id: id.into(),
        };
        let devices = [device("a"), device("b")];
        // Entry 0 is the Windows default.
        assert_eq!(mic_device_id(0, &devices), None);
        assert_eq!(mic_device_id(1, &devices).as_deref(), Some("a"));
        assert_eq!(mic_device_id(2, &devices).as_deref(), Some("b"));
        assert_eq!(mic_device_id(3, &devices), None, "past the end");
        assert_eq!(mic_index_of(None, &devices), 0);
        assert_eq!(mic_index_of(Some("b"), &devices), 2);
        assert_eq!(mic_index_of(Some("gone"), &devices), 0);
        for index in 0..=2 {
            let id = mic_device_id(index, &devices);
            assert_eq!(mic_index_of(id.as_deref(), &devices), index);
        }
    }
}
