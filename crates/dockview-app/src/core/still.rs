//! Taking a picture of the game (see `still_service`).

use super::*;

impl Core {
    pub(super) fn take_still(&mut self) {
        self.stills.request(&self.slot);
    }

    pub(super) fn poll_still(&mut self, now: Instant) {
        if let Some(event) = self.stills.poll(&self.slot, now) {
            self.emit(event);
        }
    }
}
