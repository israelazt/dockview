//! Dockview's domain: the rules and the data, and nothing that touches the
//! machine. No Windows, no threads of its own, no disk, no window.
//!
//! Everything that does touch the machine lives in `dockview-platform` and
//! meets this crate through the ports defined here. See `rust/ARCHITECTURE.md`.

pub mod audio_status;
pub mod audio_sync;
pub mod device;
pub mod device_names;
pub mod diagnostics;
pub mod encoded;
pub mod events;
pub mod frame;
pub mod geometry;
pub mod i18n;
pub mod pcm;
pub mod ports;
pub mod recording;
pub mod replay;
pub mod scene;
pub mod settings;
pub mod stream;
pub mod theme;
pub mod transition;
pub mod view;
