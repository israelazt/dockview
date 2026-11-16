//! Reading text from the clipboard (to paste a stream key into a field).

use windows::Win32::Foundation::HGLOBAL;
use windows::Win32::System::DataExchange::{CloseClipboard, GetClipboardData, OpenClipboard};
use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};

/// `CF_UNICODETEXT`: UTF-16 text ending in a zero.
const CF_UNICODETEXT: u32 = 13;
/// A pasted text is never this long; a block without an end stops here.
const MAX_UNITS: usize = 1 << 16;

/// The text on the clipboard, if there is any.
pub fn read() -> Option<String> {
    // SAFETY: the clipboard is opened and closed in this function; the block
    // it hands out is locked while it is read, and the read stops at the zero
    // that ends the text (or at `MAX_UNITS` for a damaged block).
    unsafe {
        OpenClipboard(None).ok()?;
        let text = (|| {
            let handle = GetClipboardData(CF_UNICODETEXT).ok()?;
            let block = HGLOBAL(handle.0);
            let at = GlobalLock(block).cast::<u16>().cast_const();
            if at.is_null() {
                return None;
            }
            let mut len = 0;
            while len < MAX_UNITS && *at.add(len) != 0 {
                len += 1;
            }
            let text = String::from_utf16_lossy(std::slice::from_raw_parts(at, len));
            let _ = GlobalUnlock(block);
            Some(text)
        })();
        let _ = CloseClipboard();
        text
    }
}
