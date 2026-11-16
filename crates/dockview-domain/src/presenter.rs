//! Putting the video and the interface on the window: what the application's
//! window needs from a graphics system, whichever it is (Direct3D on Windows,
//! Vulkan or OpenGL on Linux). The window's code builds one frame of interface
//! as a [`DrawList`] and hands it to a [`Presenter`], which draws the video, the
//! interface on top, and shows it.

use std::fmt;

use crate::draw::DrawList;
use crate::frame::Frame;
use crate::layout::Rect;
use crate::render_plan::ScaleFilter;

/// What a presenter could not do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresentError {
    pub message: String,
    /// The graphics device is gone (a driver update or reset): the presenter
    /// has to be built again.
    pub device_lost: bool,
}

impl PresentError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            device_lost: false,
        }
    }

    /// An error that says the device is lost.
    pub fn lost(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            device_lost: true,
        }
    }
}

impl fmt::Display for PresentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for PresentError {}

/// What was on the window, as RGBA (for the `--screenshot` of the program).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Screenshot {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// The window's graphics.
pub trait Presenter {
    /// The size of what is drawn on, in physical pixels.
    fn size(&self) -> (u32, u32);
    /// The window changed size.
    fn resize(&mut self, width: u32, height: u32) -> Result<(), PresentError>;
    /// How the video is scaled to the window.
    fn set_filter(&mut self, filter: ScaleFilter);
    /// Where the video goes (`None` is the whole window), in physical pixels.
    fn set_video_area(&mut self, area: Option<Rect>);
    /// Waits (up to `timeout_ms`) until the system can take another picture, so
    /// the freshest frame can be chosen just before it is drawn.
    fn wait_for_slot(&self, timeout_ms: u32);
    /// Makes `frame` the picture to show.
    fn upload(&mut self, frame: &Frame) -> Result<(), PresentError>;
    /// Draws the video and then `ui` on top, and shows it. With `capture`, also
    /// returns what was drawn.
    fn present(
        &mut self,
        ui: &DrawList<'_>,
        capture: bool,
    ) -> Result<Option<Screenshot>, PresentError>;
    /// Time the GPU took to draw the last frames, in milliseconds.
    fn gpu_ms(&self) -> Option<f32>;
    /// The graphics device was lost: the presenter is of no use any more.
    fn device_lost(&self) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_error_says_what_failed_and_whether_the_device_is_gone() {
        let e = PresentError::new("no swap chain");
        assert_eq!(e.to_string(), "no swap chain");
        assert!(!e.device_lost);
        assert!(PresentError::lost("removed").device_lost);
    }
}
