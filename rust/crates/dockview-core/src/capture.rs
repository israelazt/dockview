//! Threaded video capture from a UVC device through a Media Foundation source
//! reader, publishing the newest frame into a [`FrameSlot`].
//!
//! Low latency choices:
//! * `MF_LOW_LATENCY` disables decoder look-ahead.
//! * One dedicated thread calls `ReadSample` synchronously.
//! * Frames go through a single-slot mailbox, never a queue.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use windows::core::{Error, Result};
use windows::Win32::Foundation::E_FAIL;
use windows::Win32::Media::MediaFoundation::{
    IMFAttributes, IMFSample, IMFSourceReader, MFCreateAttributes, MFCreateMediaType,
    MFCreateSourceReaderFromMediaSource, MFMediaType_Video, MFVideoFormat_NV12, MFVideoFormat_YUY2,
    MF_LOW_LATENCY, MF_MT_DEFAULT_STRIDE, MF_MT_FRAME_SIZE, MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE,
    MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED, MF_SOURCE_READERF_ENDOFSTREAM,
    MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING,
};

use crate::frame::{Frame, FrameSlot, PixelFormat};
use crate::mf::{first_video_stream, open_source, read_native_types, unpack_pair, MfSession};
use crate::stream::{select_best, select_best_compressed, CaptureProfile, StreamInfo};

/// How long `Drop` waits for the capture thread before detaching it. A source
/// with no signal can leave `ReadSample` blocked indefinitely.
const STOP_TIMEOUT: Duration = Duration::from_secs(2);

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

/// Handle to a running capture thread. Dropping it stops the capture.
pub struct VideoCapture {
    stop: Arc<AtomicBool>,
    status: Arc<Mutex<CaptureStatus>>,
    thread: Option<JoinHandle<()>>,
}

impl VideoCapture {
    /// Starts capturing from the device identified by `symbolic_link`.
    pub fn start(symbolic_link: String, options: CaptureOptions, slot: Arc<FrameSlot>) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let status = Arc::new(Mutex::new(CaptureStatus::Starting));

        let thread = {
            let (stop, status) = (Arc::clone(&stop), Arc::clone(&status));
            thread::Builder::new()
                .name("video-capture".into())
                .spawn(move || run(&symbolic_link, options, &slot, &stop, &status))
                .ok()
        };
        if thread.is_none() {
            set_status(
                &status,
                CaptureStatus::Failed("could not spawn capture thread".into()),
            );
        }

        Self {
            stop,
            status,
            thread,
        }
    }

    pub fn status(&self) -> CaptureStatus {
        self.status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

impl Drop for VideoCapture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let Some(thread) = self.thread.take() else {
            return;
        };
        let deadline = Instant::now() + STOP_TIMEOUT;
        while !thread.is_finished() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        if thread.is_finished() {
            let _ = thread.join();
        }
        // Otherwise detach: the thread exits on its own once ReadSample returns.
    }
}

fn set_status(status: &Mutex<CaptureStatus>, value: CaptureStatus) {
    *status.lock().unwrap_or_else(|e| e.into_inner()) = value;
}

/// Thread entry point: runs the capture and records how it ended.
fn run(
    symbolic_link: &str,
    options: CaptureOptions,
    slot: &FrameSlot,
    stop: &AtomicBool,
    status: &Mutex<CaptureStatus>,
) {
    let end = match capture(symbolic_link, options, slot, stop, status) {
        Ok(End::Stopped) => CaptureStatus::Stopped,
        Ok(End::StreamEnded) => CaptureStatus::Disconnected {
            reason: "end of stream".into(),
        },
        Err(Failure::Setup(e)) => CaptureStatus::Failed(e.message()),
        Err(Failure::Streaming(e)) => CaptureStatus::Disconnected {
            reason: e.message(),
        },
    };
    set_status(status, end);
}

enum End {
    Stopped,
    StreamEnded,
}

enum Failure {
    /// Opening or configuring the device failed.
    Setup(Error),
    /// Reading frames failed after the stream was running (device lost).
    Streaming(Error),
}

impl From<Error> for Failure {
    fn from(e: Error) -> Self {
        Self::Setup(e)
    }
}

fn capture(
    symbolic_link: &str,
    options: CaptureOptions,
    slot: &FrameSlot,
    stop: &AtomicBool,
    status: &Mutex<CaptureStatus>,
) -> std::result::Result<End, Failure> {
    // Declared first so it is dropped last, after the reader and the source.
    let _session = MfSession::start()?;
    let (activate, source) = open_source(symbolic_link)?;

    // SAFETY: `source` is a valid capture source; `read_frames` upholds the
    // documented source-reader call order and releases its COM objects before
    // the source is shut down below.
    let outcome = unsafe {
        let reader = create_reader(&source)?;
        read_frames(&reader, options, slot, stop, status)
    };
    // SAFETY: releases the capture source opened above.
    unsafe {
        let _ = activate.ShutdownObject();
    }
    outcome
}

/// # Safety
/// `source` must be a valid media source.
unsafe fn create_reader(
    source: &windows::Win32::Media::MediaFoundation::IMFMediaSource,
) -> Result<IMFSourceReader> {
    let mut attrs: Option<IMFAttributes> = None;
    MFCreateAttributes(&mut attrs, 2)?;
    let attrs = attrs.ok_or_else(|| Error::new(E_FAIL, "MFCreateAttributes returned null"))?;
    attrs.SetUINT32(&MF_LOW_LATENCY, 1)?;
    // Lets the reader insert the decoder / colour converter when the device's
    // native format (for example MJPEG) is not what we ask for.
    attrs.SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1)?;
    MFCreateSourceReaderFromMediaSource(source, &attrs)
}

/// # Safety
/// `reader` must be a valid source reader for a video capture source.
unsafe fn read_frames(
    reader: &IMFSourceReader,
    options: CaptureOptions,
    slot: &FrameSlot,
    stop: &AtomicBool,
    status: &Mutex<CaptureStatus>,
) -> std::result::Result<End, Failure> {
    let stream = first_video_stream();

    let types = read_native_types(reader)?;
    let has_compressed = types.iter().any(|t| t.is_compressed);
    let best = match options.mode_index {
        Some(index) => types.iter().find(|t| t.type_index == index).cloned(),
        None if options.prefer_compressed => select_best_compressed(&types, options.profile)
            .or_else(|| select_best(&types, options.profile).cloned()),
        None => select_best(&types, options.profile).cloned(),
    }
    .ok_or_else(|| Error::new(E_FAIL, "requested video mode is not available"))?;

    // 1. Pick the device's native mode.
    let native = reader.GetNativeMediaType(stream, best.type_index)?;
    reader.SetCurrentMediaType(stream, None, &native)?;

    // 2. Ask for an output format the renderer understands.
    let output = output_format_for(&best);
    let subtype = match output {
        PixelFormat::Nv12 => MFVideoFormat_NV12,
        PixelFormat::Yuy2 => MFVideoFormat_YUY2,
    };
    let requested = MFCreateMediaType()?;
    requested.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
    requested.SetGUID(&MF_MT_SUBTYPE, &subtype)?;
    requested.SetUINT64(
        &MF_MT_FRAME_SIZE,
        (u64::from(best.width) << 32) | u64::from(best.height),
    )?;
    reader.SetCurrentMediaType(stream, None, &requested)?;

    let mut layout = current_layout(reader, stream, output)?;
    set_status(
        status,
        CaptureStatus::Running {
            native: best,
            output,
            has_compressed,
        },
    );

    while !stop.load(Ordering::Relaxed) {
        let mut flags = 0u32;
        let mut timestamp = 0i64;
        let mut sample: Option<IMFSample> = None;
        reader
            .ReadSample(
                stream,
                0,
                None,
                Some(&mut flags),
                Some(&mut timestamp),
                Some(&mut sample),
            )
            .map_err(Failure::Streaming)?;

        if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
            return Ok(End::StreamEnded);
        }
        if flags & MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED.0 as u32 != 0 {
            layout = current_layout(reader, stream, output).map_err(Failure::Streaming)?;
        }
        let Some(sample) = sample else {
            continue; // stream tick or gap: no data this round
        };

        let buffer = sample
            .ConvertToContiguousBuffer()
            .map_err(Failure::Streaming)?;
        let mut data: *mut u8 = std::ptr::null_mut();
        let mut len = 0u32;
        buffer
            .Lock(&mut data, None, Some(&mut len))
            .map_err(Failure::Streaming)?;
        if !data.is_null() {
            // SAFETY: Lock guarantees `len` readable bytes until Unlock.
            let bytes = std::slice::from_raw_parts(data, len as usize);
            publish_frame(slot, &layout, output, bytes, timestamp);
        }
        let _ = buffer.Unlock();
    }
    Ok(End::Stopped)
}

/// Copies `bytes` into a recycled frame and publishes it. Truncated buffers
/// are ignored rather than shown half-drawn.
fn publish_frame(
    slot: &FrameSlot,
    layout: &Layout,
    format: PixelFormat,
    bytes: &[u8],
    timestamp: i64,
) {
    let expected = format.buffer_len(layout.stride as usize, layout.height as usize);
    if bytes.len() < expected {
        return;
    }
    let mut frame = slot.take_spare().unwrap_or_else(|| Frame {
        width: 0,
        height: 0,
        stride: 0,
        format,
        data: Vec::with_capacity(expected),
        timestamp: 0,
        arrived: std::time::Instant::now(),
    });
    frame.width = layout.width;
    frame.height = layout.height;
    frame.stride = layout.stride;
    frame.format = format;
    frame.timestamp = timestamp;
    frame.data.clear();
    frame.data.extend_from_slice(&bytes[..expected]);
    slot.publish(frame);
}

/// Negotiated frame geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Layout {
    width: u32,
    height: u32,
    stride: u32,
}

/// # Safety
/// `reader` must be a valid source reader.
unsafe fn current_layout(
    reader: &IMFSourceReader,
    stream: u32,
    format: PixelFormat,
) -> Result<Layout> {
    let media_type = reader.GetCurrentMediaType(stream)?;
    let (width, height) = unpack_pair(media_type.GetUINT64(&MF_MT_FRAME_SIZE)?);
    let reported = media_type.GetUINT32(&MF_MT_DEFAULT_STRIDE).ok();
    Ok(Layout {
        width,
        height,
        stride: resolve_stride(reported, format, width),
    })
}

/// Renderer-friendly output format for a device mode: YUY2 stays YUY2,
/// everything else (including MJPEG after decoding) is delivered as NV12.
fn output_format_for(mode: &StreamInfo) -> PixelFormat {
    if mode.format_name == "YUY2" {
        PixelFormat::Yuy2
    } else {
        PixelFormat::Nv12
    }
}

/// Row stride in bytes. `MF_MT_DEFAULT_STRIDE` is a signed value (negative
/// means bottom-up); only its magnitude matters here. Falls back to the
/// tightly packed stride when the attribute is missing or zero.
fn resolve_stride(reported: Option<u32>, format: PixelFormat, width: u32) -> u32 {
    let magnitude = reported.map_or(0, |s| (s as i32).unsigned_abs());
    if magnitude > 0 {
        magnitude
    } else {
        match format {
            PixelFormat::Nv12 => width,
            PixelFormat::Yuy2 => width * 2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mode(name: &str) -> StreamInfo {
        StreamInfo {
            type_index: 0,
            format_name: name.into(),
            width: 1920,
            height: 1080,
            fps: 30,
            is_compressed: name == "MJPEG",
        }
    }

    #[test]
    fn yuy2_stays_yuy2_and_other_formats_become_nv12() {
        assert_eq!(output_format_for(&mode("YUY2")), PixelFormat::Yuy2);
        assert_eq!(output_format_for(&mode("NV12")), PixelFormat::Nv12);
        assert_eq!(output_format_for(&mode("MJPEG")), PixelFormat::Nv12);
    }

    #[test]
    fn stride_uses_reported_magnitude() {
        assert_eq!(resolve_stride(Some(2048), PixelFormat::Nv12, 1920), 2048);
        assert_eq!(
            resolve_stride(Some((-2048i32) as u32), PixelFormat::Nv12, 1920),
            2048
        );
    }

    #[test]
    fn stride_falls_back_to_packed_width() {
        assert_eq!(resolve_stride(None, PixelFormat::Nv12, 1920), 1920);
        assert_eq!(resolve_stride(Some(0), PixelFormat::Yuy2, 1920), 3840);
    }

    #[test]
    fn truncated_buffers_are_not_published() {
        let slot = FrameSlot::new();
        let layout = Layout {
            width: 4,
            height: 2,
            stride: 4,
        };
        publish_frame(&slot, &layout, PixelFormat::Nv12, &[0; 11], 0);
        assert_eq!(slot.published(), 0);
    }

    #[test]
    fn full_buffers_are_published_and_trimmed_to_size() {
        let slot = FrameSlot::new();
        let layout = Layout {
            width: 4,
            height: 2,
            stride: 4,
        };
        publish_frame(&slot, &layout, PixelFormat::Nv12, &[7; 20], 42);
        let frame = slot.take_latest().unwrap();
        assert_eq!(frame.data.len(), 12);
        assert_eq!((frame.width, frame.height, frame.stride), (4, 2, 4));
        assert_eq!(frame.timestamp, 42);
    }
}
