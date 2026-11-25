//! A fake of where the pictures of the game go.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

use dockview_domain::frame::{ColorInfo, Frame, PixelFormat};
use dockview_domain::image::{Image, ImageFormat};
use dockview_domain::ports::{ImageOutput, StillOutput};

/// Notes every picture written and copied; each can be made to fail.
#[derive(Default)]
pub struct FakeStills {
    /// `(path, width, height)` of each file written.
    pub saved: Mutex<Vec<(PathBuf, u32, u32)>>,
    pub copied: Mutex<u32>,
    pub fail_save: bool,
    pub fail_copy: bool,
}

impl ImageOutput for FakeStills {
    fn save(&self, image: &Image, path: &Path, _: ImageFormat) -> Result<(), String> {
        if self.fail_save {
            return Err("disk full".into());
        }
        let entry = (path.to_owned(), image.width, image.height);
        self.saved.lock().unwrap().push(entry);
        Ok(())
    }
    fn copy(&self, _: &Image) -> Result<(), String> {
        if self.fail_copy {
            return Err("the clipboard is busy".into());
        }
        *self.copied.lock().unwrap() += 1;
        Ok(())
    }
}

impl StillOutput for FakeStills {
    fn folder(&self) -> PathBuf {
        PathBuf::from("no-such-folder")
    }
    fn clock(&self) -> ((u16, u16, u16), (u16, u16, u16)) {
        ((2026, 11, 25), (14, 30, 12))
    }
}

/// A small frame of video.
pub fn frame() -> Frame {
    Frame {
        width: 4,
        height: 2,
        stride: 4,
        format: PixelFormat::Nv12,
        color: ColorInfo::default(),
        data: vec![128; 12],
        timestamp: 0,
        arrived: Instant::now(),
    }
}
