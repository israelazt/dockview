//! A fake of going live: it notes what it was asked and lets a test choose what
//! the stream reports.

use std::sync::{Arc, Mutex};

use dockview_domain::broadcast::{
    MemorySecrets, SecretStore, StreamHandle, StreamMachine, StreamRequest, StreamState,
    StreamStats,
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
    /// The counters running streams report.
    pub sent_frames: u64,
    pub dropped_frames: u64,
    pub bitrate_bps: u32,
    /// Streams running now.
    pub alive: u32,
    /// The queues the last stream hands out for sound.
    pub audio: Option<Arc<AudioTap>>,
    pub mic: Option<Arc<AudioTap>>,
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
        let (audio, mic) = (Arc::new(AudioTap::new(4)), Arc::new(AudioTap::new(4)));
        world.audio = Some(Arc::clone(&audio));
        world.mic = Some(Arc::clone(&mic));
        Ok(Box::new(FakeHandle {
            world: Arc::clone(&self.0),
            audio,
            mic,
        }))
    }
}

impl StreamHandle for FakeHandle {
    fn stats(&self) -> StreamStats {
        let world = self.world.lock().unwrap();
        StreamStats {
            state: world.state,
            sent_frames: world.sent_frames,
            dropped_frames: world.dropped_frames,
            bitrate_bps: world.bitrate_bps,
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

/// Secrets the test keeps a second handle to.
pub struct SharedSecrets(pub Arc<MemorySecrets>);

impl SecretStore for SharedSecrets {
    fn get(&self, name: &str) -> Option<String> {
        self.0.get(name)
    }

    fn set(&self, name: &str, value: &str) -> Result<(), String> {
        self.0.set(name, value)
    }

    fn remove(&self, name: &str) {
        self.0.remove(name);
    }
}
