//! A console for a program that has none of its own.
//!
//! The release build is a window program: double-clicking it shows no black
//! console window. Started from a terminal, though, it should still print
//! (`--screenshot`, `--log-stats`, the log's mirror), so it attaches to the
//! terminal that started it.

use windows::core::w;
use windows::Win32::Foundation::{GENERIC_WRITE, HANDLE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Console::{
    AttachConsole, GetStdHandle, SetStdHandle, ATTACH_PARENT_PROCESS, STD_ERROR_HANDLE, STD_HANDLE,
    STD_OUTPUT_HANDLE,
};

/// Whether a standard handle cannot be written to (a window program starts with
/// none), as opposed to a real one (a file or a pipe the output was redirected
/// to, which must be left alone).
fn is_unusable(handle: &windows::core::Result<HANDLE>) -> bool {
    handle
        .as_ref()
        .map_or(true, |h| h.is_invalid() || h.0.is_null())
}

/// Attaches to the console of the terminal that started the program, and
/// points the standard output and error at it unless they were redirected.
/// Returns whether it attached; it does not when the program has a console
/// already (a debug build) or was not started from one (a double click).
pub fn attach_parent_console() -> bool {
    // SAFETY: plain console calls; the handle opened here is handed to the
    // process as its standard handle and stays open for its whole life.
    unsafe {
        if AttachConsole(ATTACH_PARENT_PROCESS).is_err() {
            return false;
        }
        for which in [STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
            if is_unusable(&GetStdHandle(which)) {
                point_at_console(which);
            }
        }
    }
    true
}

/// # Safety
/// The process must have a console.
unsafe fn point_at_console(which: STD_HANDLE) {
    if let Ok(console) = CreateFileW(
        w!("CONOUT$"),
        GENERIC_WRITE.0,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
        None,
        OPEN_EXISTING,
        FILE_ATTRIBUTE_NORMAL,
        None,
    ) {
        let _ = SetStdHandle(which, console);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Foundation::INVALID_HANDLE_VALUE;

    #[test]
    fn a_missing_or_null_handle_is_unusable_and_a_real_one_is_left_alone() {
        assert!(is_unusable(&Err(windows::core::Error::empty())));
        assert!(is_unusable(&Ok(HANDLE(std::ptr::null_mut()))));
        assert!(is_unusable(&Ok(INVALID_HANDLE_VALUE)));
        assert!(!is_unusable(&Ok(HANDLE(0x1234 as *mut _))));
    }
}
