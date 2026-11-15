//! A fake of going live: it notes what it was asked and lets a test choose what
//! the stream reports.

use std::sync::{Arc, Mutex};

use dockview_domain::broadcast::{
    StreamHandle, StreamMachine, StreamRequest, StreamState, StreamStats,
};
use dockview_domain::frame::FrameSlot;
use dockview_domain::pcm::AudioTap;

#[derive(Default)]
pub struct StreamWorld {
    /// Every stream that was started, in order.
    pub requests: Vec<StreamRequest>,
    /// Make starting fail with this message.
    pub refuse: Option<String>,
    /// What running streams report.
    pub state: StreamState,
    pub error: Option<String>,
    /// Streams running now.
    pub alive: u32,
}

pub type Shared = Arc<Mutex<StreamWorld>>;

pub fn world() -> Shared {
    Arc::default()
}

pub struct FakeStream(pub Shared);

struct FakeHandle {
    world: Shared,
    audio: Arc<AudioTap>,
    mic: Arc<AudioTap>,
}

impl StreamMachine for FakeStream {
    fn start_stream(
        &self,
        _slot: Arc<FrameSlot>,
        request: StreamRequest,
    ) -> Result<Box<dyn StreamHandle>, String> {
        let mut world = self.0.lock().unwrap();
        if let Some(why) = world.refuse.clone() {
            return Err(why);
        }
        world.requests.push(request);
        world.alive += 1;
        world.state = StreamState::Connecting;
        world.error = None;
        Ok(Box::new(FakeHandle {
            world: Arc::clone(&self.0),
            audio: Arc::new(AudioTap::new(4)),
            mic: Arc::new(AudioTap::new(4)),
        }))
    }
}

impl StreamHandle for FakeHandle {
    fn stats(&self) -> StreamStats {
        StreamStats {
            state: self.world.lock().unwrap().state,
            ..StreamStats::default()
        }
    }

    fn last_error(&self) -> Option<String> {
        self.world.lock().unwrap().error.clone()
    }

    fn audio_tap(&self) -> Arc<AudioTap> {
        Arc::clone(&self.audio)
    }

    fn mic_tap(&self) -> Arc<AudioTap> {
        Arc::clone(&self.mic)
    }
}

impl Drop for FakeHandle {
    fn drop(&mut self) {
        self.world.lock().unwrap().alive -= 1;
    }
}
