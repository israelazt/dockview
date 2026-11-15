//! The operating system as the window sees it: the few things the shell needs
//! that are not portable (the native window, a message box, the "show
//! animations" setting, a folder picker, the graphics, the screenshot tool).
//! `windows.rs` is the real thing; `other.rs` answers "not available" so the
//! program builds elsewhere (Linux is a goal of 1.0) while its adapters are
//! written. The rest of the shell knows nothing about Windows.

mod clock;
#[cfg(not(windows))]
mod other;
#[cfg(windows)]
mod windows;

#[cfg(not(windows))]
pub use other::*;
#[cfg(windows)]
pub use windows::*;

/// The native handle of the window, as the system's own number (a `HWND` on
/// Windows), for the calls that need one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeWindow(pub isize);
