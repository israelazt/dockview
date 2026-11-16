//! Media Foundation access: video device and native stream-type enumeration.
//!
//! All calls require an initialised [`MfSession`], which keeps COM (MTA) and
//! Media Foundation alive for the thread that created it.

use std::marker::PhantomData;

use windows::core::{Error, Result, GUID, PWSTR};
use windows::Win32::Foundation::E_FAIL;
use windows::Win32::Media::MediaFoundation::{
    IMFActivate, IMFAttributes, IMFMediaSource, IMFSourceReader, MFCreateAttributes,
    MFCreateSourceReaderFromMediaSource, MFEnumDeviceSources, MFShutdown, MFStartup,
    MFVideoFormat_MJPG, MFVideoFormat_NV12, MFVideoFormat_YUY2, MFSTARTUP_NOSOCKET,
    MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME, MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
    MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
    MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK, MF_E_NO_MORE_TYPES, MF_MT_FRAME_RATE,
    MF_MT_FRAME_SIZE, MF_MT_SUBTYPE, MF_SOURCE_READER_FIRST_VIDEO_STREAM, MF_VERSION,
};
use windows::Win32::System::Com::{
    CoInitializeEx, CoTaskMemFree, CoUninitialize, COINIT_MULTITHREADED,
};

use dockview_domain::device::VideoDevice;
use dockview_domain::stream::StreamInfo;

/// Frame rate assumed when a media type does not report a valid one.
const DEFAULT_FPS: u32 = 30;

/// RAII guard that initialises COM (multithreaded) and Media Foundation on the
/// current thread. It is intentionally `!Send`: create one per thread that
/// talks to Media Foundation.
pub struct MfSession {
    uninit_com: bool,
    _not_send: PhantomData<*const ()>,
}

impl MfSession {
    /// Initialises COM in the MTA and starts Media Foundation.
    pub fn start() -> Result<Self> {
        // SAFETY: plain FFI initialisation calls with valid arguments.
        unsafe {
            let hr = CoInitializeEx(None, COINIT_MULTITHREADED);
            // S_OK / S_FALSE both require a matching CoUninitialize. Failure
            // (e.g. the thread is already STA) means we must not call it.
            let uninit_com = hr.is_ok();
            if let Err(e) = MFStartup(MF_VERSION, MFSTARTUP_NOSOCKET) {
                if uninit_com {
                    CoUninitialize();
                }
                return Err(e);
            }
            Ok(Self {
                uninit_com,
                _not_send: PhantomData,
            })
        }
    }
}

impl Drop for MfSession {
    fn drop(&mut self) {
        // SAFETY: balanced with the calls in `start`.
        unsafe {
            let _ = MFShutdown();
            if self.uninit_com {
                CoUninitialize();
            }
        }
    }
}

/// Lists the active video capture devices.
pub fn enumerate_video_devices(_session: &MfSession) -> Result<Vec<VideoDevice>> {
    let activates = video_capture_activates()?;
    Ok(activates
        .iter()
        .filter_map(|activate| {
            let name = get_string(activate, &MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME)?;
            if name.trim().is_empty() {
                return None;
            }
            let symbolic_link = get_string(
                activate,
                &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK,
            )
            .unwrap_or_default();
            Some(VideoDevice {
                friendly_name: name,
                symbolic_link,
            })
        })
        .collect())
}

/// Lists every native stream type the device with `symbolic_link` exposes.
pub fn enumerate_stream_types(
    _session: &MfSession,
    symbolic_link: &str,
) -> Result<Vec<StreamInfo>> {
    let (activate, source) = open_source(symbolic_link)?;
    // SAFETY: `source` is a valid media source for a video capture device.
    let result = unsafe {
        let reader = MFCreateSourceReaderFromMediaSource(&source, None::<&IMFAttributes>)?;
        read_native_types(&reader)
    };
    // SAFETY: releases the capture source opened above.
    unsafe {
        let _ = activate.ShutdownObject();
    }
    result
}

/// Finds the video capture device with `symbolic_link` and activates its
/// media source. Keep the returned activate alive while using the source and
/// call `ShutdownObject` on it when finished.
pub(crate) fn open_source(symbolic_link: &str) -> Result<(IMFActivate, IMFMediaSource)> {
    let activate = video_capture_activates()?
        .into_iter()
        .find(|a| {
            get_string(a, &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK).as_deref()
                == Some(symbolic_link)
        })
        .ok_or_else(|| Error::new(E_FAIL, "video device not found"))?;
    // SAFETY: `activate` is a valid IMFActivate for a video capture source.
    let source: IMFMediaSource = unsafe { activate.ActivateObject()? };
    Ok((activate, source))
}

/// Index of the first video stream, as the u32 the reader APIs expect.
pub(crate) fn first_video_stream() -> u32 {
    MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32
}

/// Reads every native media type of the first video stream.
///
/// # Safety
/// `reader` must be a valid source reader for a video capture source.
pub(crate) unsafe fn read_native_types(reader: &IMFSourceReader) -> Result<Vec<StreamInfo>> {
    let stream = first_video_stream();
    let mut types = Vec::new();
    for index in 0u32.. {
        let media_type = match reader.GetNativeMediaType(stream, index) {
            Ok(t) => t,
            Err(e) if e.code() == MF_E_NO_MORE_TYPES => break,
            Err(e) => return Err(e),
        };

        let Ok(subtype) = media_type.GetGUID(&MF_MT_SUBTYPE) else {
            continue;
        };
        let Ok(size) = media_type.GetUINT64(&MF_MT_FRAME_SIZE) else {
            continue;
        };
        let (width, height) = unpack_pair(size);
        let fps = media_type
            .GetUINT64(&MF_MT_FRAME_RATE)
            .map(|rate| {
                let (num, den) = unpack_pair(rate);
                fps_from_rate(num, den)
            })
            .unwrap_or(DEFAULT_FPS);

        types.push(StreamInfo {
            type_index: index,
            format_name: format_name(&subtype),
            width,
            height,
            fps,
            is_compressed: is_compressed(&subtype),
        });
    }
    Ok(types)
}

/// Enumerates video capture sources. The returned activates own their COM
/// references and release them on drop.
fn video_capture_activates() -> Result<Vec<IMFActivate>> {
    // SAFETY: the attribute store, output pointers and CoTaskMem array follow
    // the documented MFEnumDeviceSources contract.
    unsafe {
        let mut attrs: Option<IMFAttributes> = None;
        MFCreateAttributes(&mut attrs, 1)?;
        let attrs = attrs.ok_or_else(|| Error::new(E_FAIL, "MFCreateAttributes returned null"))?;
        attrs.SetGUID(
            &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
            &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
        )?;

        let mut array: *mut Option<IMFActivate> = std::ptr::null_mut();
        let mut count = 0u32;
        MFEnumDeviceSources(&attrs, &mut array, &mut count)?;
        if array.is_null() {
            return Ok(Vec::new());
        }

        let slots = std::slice::from_raw_parts_mut(array, count as usize);
        let activates = slots.iter_mut().filter_map(Option::take).collect();
        CoTaskMemFree(Some(array as *const _));
        Ok(activates)
    }
}

/// Reads a string attribute, freeing the CoTaskMem buffer Media Foundation
/// allocated for it.
pub(crate) fn get_string(attrs: &IMFAttributes, key: &GUID) -> Option<String> {
    // SAFETY: GetAllocatedString returns a CoTaskMem-allocated, NUL-terminated
    // UTF-16 string that we copy and then free exactly once.
    unsafe {
        let mut value = PWSTR::null();
        let mut len = 0u32;
        attrs.GetAllocatedString(key, &mut value, &mut len).ok()?;
        if value.is_null() {
            return None;
        }
        let text = value.to_string().ok();
        CoTaskMemFree(Some(value.0 as *const _));
        text
    }
}

/// Splits a packed `UINT64` attribute (`high << 32 | low`) into its halves.
pub(crate) fn unpack_pair(packed: u64) -> (u32, u32) {
    ((packed >> 32) as u32, packed as u32)
}

/// Frames per second from a numerator/denominator pair, rounded; falls back
/// to [`DEFAULT_FPS`] when the denominator is zero.
fn fps_from_rate(numerator: u32, denominator: u32) -> u32 {
    if denominator == 0 {
        return DEFAULT_FPS;
    }
    (f64::from(numerator) / f64::from(denominator)).round() as u32
}

pub(crate) fn is_compressed(subtype: &GUID) -> bool {
    *subtype == MFVideoFormat_MJPG
}

/// Short display name for a media subtype, falling back to its FourCC.
fn format_name(subtype: &GUID) -> String {
    if *subtype == MFVideoFormat_MJPG {
        "MJPEG".into()
    } else if *subtype == MFVideoFormat_YUY2 {
        "YUY2".into()
    } else if *subtype == MFVideoFormat_NV12 {
        "NV12".into()
    } else {
        // Media Foundation subtypes embed the FourCC in Data1.
        subtype
            .data1
            .to_le_bytes()
            .iter()
            .map(|&b| if b.is_ascii_graphic() { b as char } else { '?' })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Media::MediaFoundation::MFVideoFormat_RGB32;

    #[test]
    fn unpacks_frame_size() {
        assert_eq!(unpack_pair((1920u64 << 32) | 1080), (1920, 1080));
    }

    #[test]
    fn fps_rounds_ntsc_rates() {
        assert_eq!(fps_from_rate(60000, 1001), 60);
        assert_eq!(fps_from_rate(30000, 1001), 30);
        assert_eq!(fps_from_rate(60, 1), 60);
    }

    #[test]
    fn zero_denominator_uses_default_fps() {
        assert_eq!(fps_from_rate(60, 0), DEFAULT_FPS);
    }

    #[test]
    fn names_known_and_unknown_formats() {
        assert_eq!(format_name(&MFVideoFormat_MJPG), "MJPEG");
        assert_eq!(format_name(&MFVideoFormat_YUY2), "YUY2");
        assert_eq!(format_name(&MFVideoFormat_NV12), "NV12");
        // RGB32 is not special-cased; its Data1 is 22, an unprintable value.
        assert_eq!(format_name(&MFVideoFormat_RGB32).chars().count(), 4);
    }

    #[test]
    fn only_mjpeg_is_compressed() {
        assert!(is_compressed(&MFVideoFormat_MJPG));
        assert!(!is_compressed(&MFVideoFormat_YUY2));
        assert!(!is_compressed(&MFVideoFormat_NV12));
    }
}
