//! Dockview's application: what the program does, without a window.
//!
//! [`Core`] owns the state and the services (the sources, the replay, the
//! scenes, the measurements), builds the [`dockview_domain::view::Model`] the
//! interface shows and carries out each [`dockview_domain::view::Action`]. It
//! works only against the ports of the domain, so it is tested with fakes. The
//! `dockview` binary gives it the real window and the real machine.

pub mod auto_switch;
pub mod cli;
mod core;
pub mod keys;
pub mod replay_service;
mod scenes_service;
mod settings_storage;
pub mod sources;
mod sources_service;
pub mod telemetry;
#[cfg(test)]
mod testing;

pub use crate::core::{Core, Ports};
