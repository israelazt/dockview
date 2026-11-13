//! Keeping the settings on disk: when to write them, and what to do when that
//! fails.
//!
//! The settings change in bursts (dragging a slider changes the volume dozens
//! of times a second), so a change only marks them as pending and they are
//! written on a later turn, at most twice a second. When a write fails it is
//! tried again at the same pace, but the user is told only once per streak of
//! failures.

use std::time::{Duration, Instant};

use dockview_domain::settings::{AppSettings, SettingsStore};

#[cfg(test)]
mod tests;

/// The least time between two writes of the settings file.
pub const SAVE_INTERVAL: Duration = Duration::from_millis(500);

pub struct SettingsStorage {
    /// Where the settings are saved (the real file, or a stand-in in tests).
    store: Box<dyn SettingsStore>,
    /// The settings changed and are not on disk yet.
    dirty: bool,
    /// The last write failed (so the user is told once, not at every try).
    failed: bool,
    /// When the settings were last written.
    written_at: Instant,
}

impl SettingsStorage {
    pub fn new(store: Box<dyn SettingsStore>) -> Self {
        Self {
            store,
            dirty: false,
            failed: false,
            written_at: Instant::now(),
        }
    }

    /// Something in the settings changed: it must be written.
    pub fn changed(&mut self) {
        self.dirty = true;
    }

    /// Whether there is something not written yet.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Forgets that anything is pending (the settings are not kept at all, as
    /// in the interface preview).
    pub fn discard(&mut self) {
        self.dirty = false;
    }

    /// Whether it is time to write: something is pending and the last write was
    /// at least [`SAVE_INTERVAL`] before `now`.
    pub fn is_due(&self, now: Instant) -> bool {
        self.dirty && now.saturating_duration_since(self.written_at) >= SAVE_INTERVAL
    }

    /// Notes that the settings were handled at `now` (written, or tried).
    pub fn mark_handled(&mut self, now: Instant) {
        self.written_at = now;
    }

    /// Writes the settings now. A failure leaves them pending, to be tried
    /// again. Returns `true` when the user should be told: the first failure of
    /// a streak.
    pub fn save(&mut self, settings: &AppSettings) -> bool {
        match self.store.save(settings) {
            Ok(()) => {
                self.dirty = false;
                self.failed = false;
                false
            }
            Err(reason) => {
                dockview_domain::log_warn!("could not save the settings: {reason}");
                let first = !self.failed;
                self.failed = true;
                first
            }
        }
    }
}
