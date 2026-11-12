//! Dockview core: capture, audio, stream selection and settings.
//! Platform-independent logic lives here so it can be unit tested.

#[cfg(windows)]
pub mod capture;
pub mod device;
pub mod frame;
#[cfg(windows)]
pub mod mf;
pub mod settings;
pub mod stream;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
