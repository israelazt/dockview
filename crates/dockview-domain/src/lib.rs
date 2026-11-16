//! Dockview's domain: the rules and the data, and nothing that touches the
//! machine. No Windows, no threads of its own, no disk, no window.
//!
//! Everything that does touch the machine lives in `dockview-platform` and
//! meets this crate through the ports defined here. See `ARCHITECTURE.md`.

pub mod audio_status;
pub mod audio_sync;
pub mod brand;
pub mod broadcast;
pub mod capture_settings;
pub mod chrome_input;
pub mod design;
pub mod desktop;
pub mod device;
pub mod device_names;
pub mod diagnostics;
pub mod draw;
pub mod encoded;
pub mod events;
pub mod frame;
pub mod geometry;
pub mod hotkey;
pub mod hud;
pub mod i18n;
pub mod image;
pub mod layer_edit;
pub mod layout;
pub mod log;
pub mod navigation;
pub mod overlay_input;
pub mod pcm;
pub mod ports;
pub mod presenter;
pub mod recording;
pub mod render_plan;
pub mod replay;
pub mod scene;
pub mod select;
pub mod settings;
pub mod settings_view;
pub mod snap;
pub mod sources;
pub mod stats;
pub mod stream;
pub mod stream_settings;
pub mod theme;
pub mod transition;
pub mod tray;
pub mod view;
pub mod window_chrome;
