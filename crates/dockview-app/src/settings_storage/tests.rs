//! The storage on its own, with a store that can be made to fail.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use dockview_domain::settings::{AppSettings, SettingsStore};

use super::{SettingsStorage, SAVE_INTERVAL};

#[derive(Default)]
struct Disk {
    saved: Mutex<Vec<AppSettings>>,
    fails: AtomicBool,
    attempts: AtomicUsize,
}

struct FakeStore(Arc<Disk>);

impl SettingsStore for FakeStore {
    fn load(&self) -> AppSettings {
        AppSettings::default()
    }
    fn save(&self, settings: &AppSettings) -> Result<(), String> {
        self.0.attempts.fetch_add(1, Ordering::Relaxed);
        if self.0.fails.load(Ordering::Relaxed) {
            return Err("disk full".into());
        }
        self.0.saved.lock().unwrap().push(settings.clone());
        Ok(())
    }
}

fn storage() -> (SettingsStorage, Arc<Disk>) {
    let disk = Arc::new(Disk::default());
    (
        SettingsStorage::new(Box::new(FakeStore(Arc::clone(&disk)))),
        disk,
    )
}

#[test]
fn nothing_is_due_until_something_changed_and_the_pause_has_passed() {
    let (mut storage, _) = storage();
    let later = Instant::now() + Duration::from_secs(10);
    assert!(!storage.is_due(later), "nothing changed");

    storage.changed();
    assert!(storage.is_dirty());
    assert!(!storage.is_due(Instant::now()), "too soon after the start");
    assert!(storage.is_due(later));
}

#[test]
fn a_write_clears_what_was_pending_and_the_pause_starts_over() {
    let (mut storage, disk) = storage();
    storage.changed();
    let now = Instant::now() + Duration::from_secs(10);
    assert!(storage.is_due(now));

    let tell = storage.save(&AppSettings::default());
    storage.mark_handled(now);

    assert!(!tell);
    assert!(!storage.is_dirty());
    assert_eq!(disk.saved.lock().unwrap().len(), 1);
    // A new change waits for the pause again.
    storage.changed();
    assert!(!storage.is_due(now + SAVE_INTERVAL / 2));
    assert!(storage.is_due(now + SAVE_INTERVAL));
}

#[test]
fn a_failed_write_stays_pending_and_is_told_once_per_streak() {
    let (mut storage, disk) = storage();
    disk.fails.store(true, Ordering::Relaxed);
    storage.changed();

    assert!(
        storage.save(&AppSettings::default()),
        "the first failure is news"
    );
    assert!(storage.is_dirty(), "it must be tried again");
    assert!(!storage.save(&AppSettings::default()), "not told again");
    assert!(!storage.save(&AppSettings::default()));
    assert_eq!(disk.attempts.load(Ordering::Relaxed), 3);

    // It works again: the streak ends, and a new failure is news again.
    disk.fails.store(false, Ordering::Relaxed);
    assert!(!storage.save(&AppSettings::default()));
    assert!(!storage.is_dirty());
    disk.fails.store(true, Ordering::Relaxed);
    storage.changed();
    assert!(storage.save(&AppSettings::default()));
}

#[test]
fn discarding_forgets_what_was_pending() {
    let (mut storage, _) = storage();
    storage.changed();
    storage.discard();
    assert!(!storage.is_dirty());
    assert!(!storage.is_due(Instant::now() + Duration::from_secs(10)));
}
