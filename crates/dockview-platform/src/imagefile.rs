//! Saving a picture to a file and putting it on the clipboard: the output half
//! of the screenshot tool. Files are written with the Windows Imaging
//! Component (the codecs Windows already has, so no library is added), and
//! the clipboard gets a bitmap every program can paste.

use std::path::Path;

use dockview_domain::image::{Image, ImageFormat};
use windows::core::{Interface, GUID, PCWSTR};
use windows::Win32::Foundation::{GENERIC_WRITE, HANDLE, HGLOBAL};
use windows::Win32::Graphics::Gdi::{BITMAPINFOHEADER, BI_RGB};
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_ContainerFormatGif, GUID_ContainerFormatJpeg,
    GUID_ContainerFormatPng, GUID_WICPixelFormat24bppBGR, GUID_WICPixelFormat32bppBGRA,
    GUID_WICPixelFormat8bppIndexed, IWICBitmapFrameEncode, IWICBitmapSource, IWICImagingFactory,
    WICBitmapDitherTypeErrorDiffusion, WICBitmapEncoderNoCache, WICBitmapPaletteTypeCustom,
};
use windows::Win32::System::Com::StructuredStorage::IPropertyBag2;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};

/// `CF_DIB`: a device-independent bitmap.
const CF_DIB: u32 = 8;

fn check(image: &Image) -> Result<(), String> {
    if image.width == 0 || image.height == 0 || !image.is_valid() {
        return Err("there is no picture to use".into());
    }
    Ok(())
}

/// Writes `image` to `path` in `format` (PNG keeps every pixel as it is; JPEG
/// drops the alpha; GIF reduces the picture to 256 colours).
pub fn save(image: &Image, path: &Path, format: ImageFormat) -> Result<(), String> {
    check(image)?;
    // SAFETY: COM calls on this thread; every interface is released on drop.
    unsafe { write(image, path, format) }.map_err(|e| format!("could not save the picture: {e}"))
}

unsafe fn write(image: &Image, path: &Path, format: ImageFormat) -> windows::core::Result<()> {
    // Already initialised on this thread is fine (S_FALSE) or in another mode
    // (the result is ignored: COM is usable either way).
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let factory: IWICImagingFactory =
        unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)? };
    let stream = unsafe { factory.CreateStream()? };
    let wide: Vec<u16> = path
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain(Some(0))
        .collect();
    unsafe { stream.InitializeFromFilename(PCWSTR(wide.as_ptr()), GENERIC_WRITE.0)? };
    let container: GUID = match format {
        ImageFormat::Png => GUID_ContainerFormatPng,
        ImageFormat::Jpeg => GUID_ContainerFormatJpeg,
        ImageFormat::Gif => GUID_ContainerFormatGif,
    };
    let encoder = unsafe { factory.CreateEncoder(&container, std::ptr::null())? };
    unsafe { encoder.Initialize(&stream, WICBitmapEncoderNoCache)? };
    let mut frame: Option<IWICBitmapFrameEncode> = None;
    let mut options: Option<IPropertyBag2> = None;
    unsafe { encoder.CreateNewFrame(&mut frame, &mut options)? };
    let frame =
        frame.ok_or_else(|| windows::core::Error::from(windows::Win32::Foundation::E_FAIL))?;
    unsafe { frame.Initialize(options.as_ref())? };
    unsafe { frame.SetSize(image.width, image.height)? };

    match format {
        ImageFormat::Png => {
            let mut pixel_format = GUID_WICPixelFormat32bppBGRA;
            unsafe { frame.SetPixelFormat(&mut pixel_format)? };
            unsafe { frame.WritePixels(image.height, image.width * 4, &image.bgra)? };
        }
        ImageFormat::Jpeg => {
            let mut pixel_format = GUID_WICPixelFormat24bppBGR;
            unsafe { frame.SetPixelFormat(&mut pixel_format)? };
            let (rows, stride) = (image.height, image.width * 3);
            let mut rgb = Vec::with_capacity(rows as usize * stride as usize);
            for px in image.bgra.as_chunks::<4>().0 {
                rgb.extend_from_slice(&px[..3]);
            }
            unsafe { frame.WritePixels(rows, stride, &rgb)? };
        }
        ImageFormat::Gif => {
            let bitmap = unsafe {
                factory.CreateBitmapFromMemory(
                    image.width,
                    image.height,
                    &GUID_WICPixelFormat32bppBGRA,
                    image.width * 4,
                    &image.bgra,
                )?
            };
            let source: IWICBitmapSource = bitmap.cast()?;
            let palette = unsafe { factory.CreatePalette()? };
            unsafe { palette.InitializeFromBitmap(&source, 256, false)? };
            let converter = unsafe { factory.CreateFormatConverter()? };
            unsafe {
                converter.Initialize(
                    &source,
                    &GUID_WICPixelFormat8bppIndexed,
                    WICBitmapDitherTypeErrorDiffusion,
                    Some(&palette),
                    0.0,
                    WICBitmapPaletteTypeCustom,
                )?
            };
            let mut pixel_format = GUID_WICPixelFormat8bppIndexed;
            unsafe { frame.SetPixelFormat(&mut pixel_format)? };
            unsafe { frame.SetPalette(&palette)? };
            let converted: IWICBitmapSource = converter.cast()?;
            unsafe { frame.WriteSource(&converted, std::ptr::null())? };
        }
    }
    unsafe { frame.Commit()? };
    unsafe { encoder.Commit()? };
    Ok(())
}

/// The bytes of a `CF_DIB`: an info header and the pixels, rows from the bottom
/// (the form every program reads), 32 bits per pixel.
pub fn dib_bytes(image: &Image) -> Vec<u8> {
    let header = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: image.width as i32,
        biHeight: image.height as i32, // positive: bottom row first
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        biSizeImage: image.bgra.len() as u32,
        ..Default::default()
    };
    let mut out = Vec::with_capacity(header.biSize as usize + image.bgra.len());
    // SAFETY: BITMAPINFOHEADER is plain old data of `biSize` bytes.
    out.extend_from_slice(unsafe {
        std::slice::from_raw_parts(
            (&header as *const BITMAPINFOHEADER).cast::<u8>(),
            header.biSize as usize,
        )
    });
    let row = image.width as usize * 4;
    for y in (0..image.height as usize).rev() {
        out.extend_from_slice(&image.bgra[y * row..(y + 1) * row]);
    }
    out
}

/// Puts `image` on the clipboard, replacing what was there.
pub fn copy_to_clipboard(image: &Image) -> Result<(), String> {
    check(image)?;
    let bytes = dib_bytes(image);
    // SAFETY: the global block is filled before the clipboard takes it, and
    // freed by the system once it owns it; the clipboard is closed on every path.
    unsafe {
        let block: HGLOBAL = GlobalAlloc(GMEM_MOVEABLE, bytes.len())
            .map_err(|e| format!("not enough memory for the picture: {e}"))?;
        let at = GlobalLock(block);
        if at.is_null() {
            return Err("could not prepare the picture".into());
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), at.cast::<u8>(), bytes.len());
        let _ = GlobalUnlock(block);
        OpenClipboard(None).map_err(|e| format!("the clipboard is busy: {e}"))?;
        let result = EmptyClipboard()
            .and_then(|()| SetClipboardData(CF_DIB, Some(HANDLE(block.0))).map(|_| ()));
        let _ = CloseClipboard();
        result.map_err(|e| format!("could not copy the picture: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("dockview-imagefile-tests");
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    /// A 16×8 picture: red on the left half, blue on the right.
    fn two_colours() -> Image {
        let mut bgra = Vec::new();
        for _ in 0..8 {
            for x in 0..16 {
                bgra.extend_from_slice(&if x < 8 {
                    [0, 0, 255, 255]
                } else {
                    [255, 0, 0, 255]
                });
            }
        }
        Image {
            width: 16,
            height: 8,
            bgra,
        }
    }

    #[test]
    fn each_format_writes_a_file_with_its_own_signature() {
        let image = two_colours();
        for (format, name, start) in [
            (ImageFormat::Png, "a.png", &b"\x89PNG"[..]),
            (ImageFormat::Jpeg, "a.jpg", &[0xFF, 0xD8][..]),
            (ImageFormat::Gif, "a.gif", &b"GIF8"[..]),
        ] {
            let path = scratch(name);
            let _ = std::fs::remove_file(&path);
            save(&image, &path, format).unwrap_or_else(|e| panic!("{format:?}: {e}"));
            let bytes = std::fs::read(&path).unwrap();
            assert!(
                bytes.starts_with(start),
                "{format:?}: {:?}",
                &bytes[..8.min(bytes.len())]
            );
            assert!(bytes.len() > 30, "{format:?} is empty");
        }
    }

    #[test]
    fn a_saved_png_has_the_size_it_was_given() {
        let path = scratch("size.png");
        save(&two_colours(), &path, ImageFormat::Png).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        // The IHDR chunk: width and height as big-endian numbers at 16 and 20.
        assert_eq!(u32::from_be_bytes(bytes[16..20].try_into().unwrap()), 16);
        assert_eq!(u32::from_be_bytes(bytes[20..24].try_into().unwrap()), 8);
    }

    #[test]
    fn nothing_is_saved_for_an_empty_or_broken_picture() {
        let path = scratch("none.png");
        assert!(save(&Image::solid(0, 0, [0; 4]), &path, ImageFormat::Png).is_err());
        let broken = Image {
            width: 4,
            height: 4,
            bgra: vec![0; 3],
        };
        assert!(save(&broken, &path, ImageFormat::Png).is_err());
        assert!(copy_to_clipboard(&broken).is_err());
    }

    #[test]
    fn a_folder_that_does_not_exist_is_an_error_not_a_panic() {
        let path = scratch("no-such-folder").join("x.png");
        assert!(save(&two_colours(), &path, ImageFormat::Png).is_err());
    }

    #[test]
    fn the_clipboard_bitmap_is_a_header_then_rows_from_the_bottom() {
        let bytes = dib_bytes(&two_colours());
        let header = std::mem::size_of::<BITMAPINFOHEADER>();
        assert_eq!(bytes.len(), header + 16 * 8 * 4);
        assert_eq!(i32::from_le_bytes(bytes[4..8].try_into().unwrap()), 16);
        assert_eq!(i32::from_le_bytes(bytes[8..12].try_into().unwrap()), 8);
        assert_eq!(u16::from_le_bytes(bytes[14..16].try_into().unwrap()), 32);
        // The first pixel stored is the bottom-left one: red (B, G, R, A).
        assert_eq!(&bytes[header..header + 4], &[0, 0, 255, 255]);

        // A picture whose top row differs from its bottom row is flipped.
        let mut image = Image::solid(1, 2, [1, 1, 1, 255]);
        image.bgra[..4].copy_from_slice(&[9, 9, 9, 255]); // the top row
        let dib = dib_bytes(&image);
        assert_eq!(
            &dib[header..header + 4],
            &[1, 1, 1, 255],
            "bottom row first"
        );
        assert_eq!(&dib[header + 4..header + 8], &[9, 9, 9, 255]);
    }

    /// Replaces the clipboard of the person running the tests, so it is only
    /// run on purpose: `cargo test -p dockview-platform -- --ignored clipboard`.
    #[test]
    #[ignore]
    fn the_clipboard_takes_the_picture() {
        copy_to_clipboard(&two_colours()).unwrap();
    }
}
