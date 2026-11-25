//! A picture of the game: the next frame of the video, saved as a PNG and put
//! on the clipboard. Only what the video has is in it, never the interface.
//! Converting and writing take a moment, so a worker thread does them.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use dockview_domain::events::AppEvent;
use dockview_domain::frame::{Frame, FrameSlot};
use dockview_domain::frame_image::image_of;
use dockview_domain::image::{screenshot_name, ImageFormat};
use dockview_domain::ports::StillOutput;

/// How long to wait for a frame before giving up (there is no video).
pub const FRAME_TIMEOUT: Duration = Duration::from_secs(2);

enum State {
    Idle,
    /// A frame was asked for; the time is noted on the first poll after.
    Waiting(Option<Instant>),
    Writing(Receiver<AppEvent>),
}

pub struct StillService {
    output: Arc<dyn StillOutput>,
    state: State,
}

impl StillService {
    pub fn new(output: Arc<dyn StillOutput>) -> Self {
        Self {
            output,
            state: State::Idle,
        }
    }

    /// Asks for a picture of what `slot` shows. While one is being taken,
    /// asking again does nothing (a held key takes one).
    pub fn request(&mut self, slot: &FrameSlot) {
        if matches!(self.state, State::Idle) {
            slot.request_still();
            self.state = State::Waiting(None);
        }
    }

    /// Moves the picture along; returns what to tell the user, once.
    pub fn poll(&mut self, slot: &FrameSlot, now: Instant) -> Option<AppEvent> {
        match &mut self.state {
            State::Idle => None,
            State::Waiting(since) => {
                let since = *since.get_or_insert(now);
                if let Some(frame) = slot.take_still() {
                    self.write(frame)
                } else if now.duration_since(since) >= FRAME_TIMEOUT {
                    slot.cancel_still();
                    self.state = State::Idle;
                    dockview_domain::log_warn!("picture of the game: no video arrived");
                    Some(AppEvent::ScreenshotFailed)
                } else {
                    None
                }
            }
            State::Writing(done) => match done.try_recv() {
                Err(TryRecvError::Empty) => None,
                result => {
                    self.state = State::Idle;
                    Some(result.unwrap_or(AppEvent::ScreenshotFailed))
                }
            },
        }
    }

    fn write(&mut self, frame: Frame) -> Option<AppEvent> {
        let (send, receive) = mpsc::channel();
        let output = Arc::clone(&self.output);
        let spawned = std::thread::Builder::new()
            .name("still".into())
            .spawn(move || {
                // Nobody listens only when the program is closing.
                let _ = send.send(deliver(&frame, output.as_ref()));
            });
        match spawned {
            Ok(_) => {
                self.state = State::Writing(receive);
                None
            }
            Err(e) => {
                dockview_domain::log_error!("picture of the game: {e}");
                self.state = State::Idle;
                Some(AppEvent::ScreenshotFailed)
            }
        }
    }
}

/// A name in `folder` that no file has yet: two pictures can share a second.
fn free_path(folder: &Path, name: &str) -> PathBuf {
    let first = folder.join(name);
    if !first.exists() {
        return first;
    }
    let (stem, extension) = name.rsplit_once('.').unwrap_or((name, "png"));
    (2..1000)
        .map(|n| folder.join(format!("{stem} ({n}).{extension}")))
        .find(|path| !path.exists())
        .unwrap_or(first)
}

/// Saves and copies the picture in `frame`. One of the two working is already
/// useful, so each is tried whatever happens to the other.
fn deliver(frame: &Frame, output: &dyn StillOutput) -> AppEvent {
    let Some(image) = image_of(frame) else {
        dockview_domain::log_error!("picture of the game: the frame cannot be read");
        return AppEvent::ScreenshotFailed;
    };
    // The clipboard first: it is what is waited for to paste.
    let copied = output.copy(&image);
    let (date, time) = output.clock();
    let name = screenshot_name(date, time, ImageFormat::Png);
    let path = free_path(&output.folder(), &name);
    let saved = output.save(&image, &path, ImageFormat::Png);
    for error in [&copied, &saved]
        .into_iter()
        .filter_map(|r| r.as_ref().err())
    {
        dockview_domain::log_error!("picture of the game: {error}");
    }
    if saved.is_ok() {
        dockview_domain::log_info!("picture of the game: saved {}", path.display());
    }
    match (saved.is_ok(), copied.is_ok()) {
        (true, true) => AppEvent::ScreenshotSavedCopied,
        (true, false) => AppEvent::ScreenshotSaved,
        (false, true) => AppEvent::ScreenshotCopied,
        (false, false) => AppEvent::ScreenshotFailed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::stills::{frame, FakeStills};

    /// Polls until the worker answers.
    fn finish(service: &mut StillService, slot: &FrameSlot) -> AppEvent {
        let start = Instant::now();
        loop {
            if let Some(event) = service.poll(slot, start) {
                return event;
            }
            assert!(start.elapsed() < Duration::from_secs(5), "no answer");
            std::thread::yield_now();
        }
    }

    fn take_with(fake: FakeStills) -> (AppEvent, Arc<FakeStills>) {
        let fake = Arc::new(fake);
        let mut service = StillService::new(Arc::clone(&fake) as Arc<_>);
        let slot = FrameSlot::new();
        service.request(&slot);
        slot.publish(frame());
        (finish(&mut service, &slot), fake)
    }

    #[test]
    fn the_next_frame_is_saved_as_png_and_copied() {
        let (event, fake) = take_with(FakeStills::default());
        assert_eq!(event, AppEvent::ScreenshotSavedCopied);
        let saved = fake.saved.lock().unwrap();
        assert_eq!(saved.len(), 1);
        assert!(saved[0].0.ends_with("Screenshot 2026-11-25 143012.png"));
        assert_eq!((saved[0].1, saved[0].2), (4, 2));
        assert_eq!(*fake.copied.lock().unwrap(), 1);
    }

    #[test]
    fn what_could_be_done_is_what_is_said() {
        let failing = |fail_save, fail_copy| FakeStills {
            fail_save,
            fail_copy,
            ..FakeStills::default()
        };
        assert_eq!(
            take_with(failing(true, false)).0,
            AppEvent::ScreenshotCopied
        );
        assert_eq!(take_with(failing(false, true)).0, AppEvent::ScreenshotSaved);
        assert_eq!(take_with(failing(true, true)).0, AppEvent::ScreenshotFailed);
    }

    #[test]
    fn without_video_it_gives_up_and_can_be_asked_again() {
        let mut service = StillService::new(Arc::new(FakeStills::default()));
        let slot = FrameSlot::new();
        service.request(&slot);
        let start = Instant::now();
        assert_eq!(service.poll(&slot, start), None);
        assert_eq!(
            service.poll(&slot, start + FRAME_TIMEOUT),
            Some(AppEvent::ScreenshotFailed)
        );
        slot.publish(frame());
        assert!(slot.take_still().is_none(), "the request is gone");
        service.request(&slot);
        slot.publish(frame());
        assert_eq!(finish(&mut service, &slot), AppEvent::ScreenshotSavedCopied);
    }

    #[test]
    fn asking_again_while_one_is_taken_takes_only_one() {
        let fake = Arc::new(FakeStills::default());
        let mut service = StillService::new(Arc::clone(&fake) as Arc<_>);
        let slot = FrameSlot::new();
        service.request(&slot);
        service.request(&slot);
        slot.publish(frame());
        assert_eq!(service.poll(&slot, Instant::now()), None);
        service.request(&slot);
        slot.publish(frame());
        finish(&mut service, &slot);
        assert_eq!(fake.saved.lock().unwrap().len(), 1);
    }

    #[test]
    fn a_taken_name_gets_a_number() {
        let folder = std::env::temp_dir().join(format!("still-names-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        assert_eq!(free_path(&folder, "a b.png"), folder.join("a b.png"));
        std::fs::write(folder.join("a b.png"), b"x").unwrap();
        assert_eq!(free_path(&folder, "a b.png"), folder.join("a b (2).png"));
        std::fs::write(folder.join("a b (2).png"), b"x").unwrap();
        assert_eq!(free_path(&folder, "a b.png"), folder.join("a b (3).png"));
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
