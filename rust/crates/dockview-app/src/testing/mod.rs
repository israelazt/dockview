//! The test rig: fakes of every port and a `Core` built on them, so the whole
//! application can be driven without a capture card, a GPU, a sound card or a
//! window. Only compiled for tests.
//!
//! Tests should use the public surface of `Core` (`apply`, `on_key`, `tick`,
//! `model`, `take_events`) and what the fakes record, never its fields, so the
//! rig survives a change of how `Core` is built inside.

pub mod replay;
pub mod rig;
pub mod sources;
pub mod system;
