//! The picture of the game, through `Core`.

use std::time::{Duration, Instant};

use dockview_domain::events::AppEvent;
use dockview_domain::view::Action;

use crate::keys::KeyCommand;
use crate::testing::rig::Rig;
use crate::testing::stills::frame;

/// Lets the application take turns until it says how the picture went.
fn outcome(rig: &mut Rig) -> AppEvent {
    let start = Instant::now();
    loop {
        rig.advance(Duration::from_millis(1));
        let found = rig.core.take_events().into_iter().find(|e| {
            matches!(
                e,
                AppEvent::ScreenshotSavedCopied | AppEvent::ScreenshotFailed
            )
        });
        if let Some(event) = found {
            return event;
        }
        assert!(start.elapsed() < Duration::from_secs(5), "no answer");
        std::thread::yield_now();
    }
}

#[test]
fn the_key_and_the_button_take_a_picture_of_the_video() {
    let mut rig = Rig::new();
    for by_key in [true, false] {
        if by_key {
            rig.core.on_key(KeyCommand::TakeStill);
        } else {
            rig.core.apply(Action::TakeStill);
        }
        rig.core.output_slot().publish(frame());
        assert_eq!(outcome(&mut rig), AppEvent::ScreenshotSavedCopied);
    }
    assert_eq!(rig.stills.saved.lock().unwrap().len(), 2);
    assert_eq!(*rig.stills.copied.lock().unwrap(), 2);
}
