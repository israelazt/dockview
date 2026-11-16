//! The data of the video sources: what a capture is asked to do, what state it
//! is in, and what can be captured. The captures themselves (threads, Media
//! Foundation, Windows Graphics Capture, the GPU compositor) are in
//! `dockview-platform`; the application sees them through the ports.

use std::sync::Arc;

use crate::frame::{FrameSlot, PixelFormat};
use crate::scene::{Fit, NormRect};
use crate::stream::{CaptureProfile, StreamInfo};

/// Lifecycle of a [`VideoCapture`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureStatus {
    /// The thread is opening the device.
    Starting,
    /// Frames are flowing. `native` is the device mode that was selected.
    Running {
        native: StreamInfo,
        output: PixelFormat,
        /// Whether the device also offers a compressed mode to fall back to.
        has_compressed: bool,
    },
    /// The device was unplugged or the stream ended.
    Disconnected { reason: String },
    /// The device could not be opened or configured.
    Failed(String),
    /// Stopped on request.
    Stopped,
}

/// Options for [`VideoCapture::start`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CaptureOptions {
    /// How to choose a device mode when `mode_index` is `None`.
    pub profile: CaptureProfile,
    /// Force a specific native mode by its `StreamInfo::type_index`.
    pub mode_index: Option<u32>,
    /// Choose among compressed (MJPEG) modes when the device has any. Used when
    /// the USB connection cannot carry an uncompressed mode.
    pub prefer_compressed: bool,
}

/// Options for a capture.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenOptions {
    /// The video is scaled down to at most this height (never up).
    pub max_height: u32,
    /// The most frames per second to deliver.
    pub max_fps: u32,
    /// Draw the mouse cursor into the video.
    pub cursor: bool,
}

impl Default for ScreenOptions {
    fn default() -> Self {
        Self {
            max_height: 1080,
            max_fps: 60,
            cursor: true,
        }
    }
}

/// State of a screen capture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScreenStatus {
    Starting,
    /// Delivering frames of this (output) size.
    Running {
        width: u32,
        height: u32,
    },
    /// What was being captured went away (the window closed).
    Ended(String),
    Failed(String),
}

/// What to capture. The numbers are the raw `HMONITOR` / `HWND` handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureTarget {
    Monitor(isize),
    Window(isize),
}

/// A monitor that can be captured.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorInfo {
    pub handle: isize,
    /// Windows' name for it, e.g. `\\.\DISPLAY1`.
    pub device_name: String,
    pub width: u32,
    pub height: u32,
    pub primary: bool,
}

/// A window that can be captured.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowInfo {
    pub handle: isize,
    pub title: String,
    pub process_id: u32,
}

/// One layer of a scene: where a source's frames are drawn.
#[derive(Clone)]
pub struct Layer {
    pub slot: Arc<FrameSlot>,
    pub rect: NormRect,
    pub fit: Fit,
    pub visible: bool,
    /// 1 is fully drawn, 0 invisible; in between it lets what is below show
    /// through (scene transitions).
    pub opacity: f32,
}

impl Layer {
    /// A source drawn over the whole picture.
    pub fn full(slot: Arc<FrameSlot>) -> Self {
        Self {
            slot,
            rect: NormRect::FULL,
            fit: Fit::Contain,
            visible: true,
            opacity: 1.0,
        }
    }
}

/// What the compositor thread reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SceneStatus {
    Starting,
    Running,
    Failed(String),
}
