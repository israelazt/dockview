//! The Multimedia Class Scheduler: Windows keeps a thread in a task class (here
//! "Pro Audio") scheduled ahead of ordinary work, so an audio thread is not
//! starved when the CPU is busy (a game and a recording running at once).
//! Microsoft recommends it for WASAPI threads.

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Threading::{
    AvRevertMmThreadCharacteristics, AvSetMmThreadCharacteristicsW,
};

/// A thread in a multimedia task class until this is dropped. It belongs to the
/// thread that made it: create it inside the thread and keep it there.
pub struct MmcssGuard {
    handle: HANDLE,
}

impl MmcssGuard {
    /// Puts the calling thread in the "Pro Audio" class. `None` when Windows
    /// does not allow it (the service is off); the thread then runs as before.
    pub fn pro_audio() -> Option<Self> {
        Self::enter(w!("Pro Audio"))
    }

    fn enter(task: PCWSTR) -> Option<Self> {
        let mut index = 0u32;
        // SAFETY: `task` is a valid, NUL-terminated string and `index` lives
        // for the call; the handle is given back in `drop`, on this thread.
        let handle = unsafe { AvSetMmThreadCharacteristicsW(task, &mut index) }.ok()?;
        Some(Self { handle })
    }
}

impl Drop for MmcssGuard {
    fn drop(&mut self) {
        // SAFETY: the handle came from `AvSetMmThreadCharacteristicsW` on this
        // thread and is released once.
        unsafe {
            let _ = AvRevertMmThreadCharacteristics(self.handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entering_and_leaving_the_audio_class_is_safe_and_can_be_repeated() {
        // The service can be off on some machines (a CI runner): then there is
        // nothing to enter, and that is fine.
        for _ in 0..3 {
            let guard = MmcssGuard::pro_audio();
            let nested = MmcssGuard::pro_audio();
            drop(nested);
            drop(guard);
        }
    }

    #[test]
    fn an_unknown_class_is_refused_instead_of_crashing() {
        assert!(MmcssGuard::enter(w!("No Such Task Class")).is_none());
    }
}
