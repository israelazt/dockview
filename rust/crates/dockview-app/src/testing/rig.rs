//! A `Core` built on fakes, with handles to everything the fakes recorded.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use dockview_domain::device::{AudioDevice, DeviceLists, VideoDevice};
use dockview_domain::frame::FrameSlot;
use dockview_domain::i18n::Lang;
use dockview_domain::settings::{AppSettings, MemoryStore, SettingsStore};
use dockview_domain::stats::Stats;

use super::replay::{self, FakeMachine};
use super::sources::{FakeSources, SharedSources};
use super::system::{FakeStats, FakeSystem, RecordingHost, SharedHost};
use crate::cli::Cli;
use crate::{Core, Ports};

/// A store the test keeps a second handle to, to see what was saved.
pub struct SharedStore(pub Arc<MemoryStore>);

impl SettingsStore for SharedStore {
    fn load(&self) -> AppSettings {
        self.0.load()
    }
    fn save(&self, settings: &AppSettings) {
        self.0.save(settings);
    }
}

/// The application and everything around it.
pub struct Rig {
    pub core: Core,
    /// What the fake replay buffer, microphone and hotkeys were asked.
    pub replay: replay::Shared,
    /// What the fake machine was asked to open, and what each source reports.
    pub sources: SharedSources,
    /// What the application asked of the window.
    pub host: SharedHost,
    /// Every measurement written to the stats log.
    pub stats: Arc<Mutex<Vec<Stats>>>,
    /// Where the settings are saved.
    pub store: Arc<MemoryStore>,
    pub system: Arc<FakeSystem>,
    /// The time the rig says it is, moved by [`Rig::advance`].
    pub now: Instant,
}

/// How a rig is set up before the application starts.
pub struct RigBuilder {
    settings: AppSettings,
    video: Vec<VideoDevice>,
    audio: Vec<AudioDevice>,
    cli: Cli,
}

impl RigBuilder {
    pub fn settings(mut self, settings: AppSettings) -> Self {
        self.settings = settings;
        self
    }

    /// The capture cards Windows lists, as `(name, symbolic link)`.
    pub fn video(mut self, devices: &[(&str, &str)]) -> Self {
        self.video = devices
            .iter()
            .map(|(name, link)| VideoDevice {
                friendly_name: (*name).into(),
                symbolic_link: (*link).into(),
            })
            .collect();
        self
    }

    /// The audio inputs Windows lists, as `(name, id)`.
    pub fn audio(mut self, devices: &[(&str, &str)]) -> Self {
        self.audio = devices
            .iter()
            .map(|(name, id)| AudioDevice {
                friendly_name: (*name).into(),
                device_id: (*id).into(),
            })
            .collect();
        self
    }

    pub fn cli(mut self, cli: Cli) -> Self {
        self.cli = cli;
        self
    }

    /// Starts the application on the fakes.
    pub fn build(self) -> Rig {
        let replay = replay::world();
        let (machine, sources) = FakeSources::new();
        sources.lock().unwrap().devices = DeviceLists {
            video: self.video,
            audio: self.audio,
        };
        let host = SharedHost::default();
        let stats = FakeStats::default();
        let stats_log = Arc::clone(&stats.0);
        let store = Arc::new(MemoryStore::new());
        let system = Arc::new(FakeSystem::default());
        let ports = Ports {
            settings: Box::new(SharedStore(Arc::clone(&store))),
            replay: Box::new(FakeMachine(Arc::clone(&replay))),
            sources: Box::new(machine),
            system: Arc::clone(&system) as Arc<_>,
            stats_sink: Some(Box::new(stats)),
            host: Box::new(RecordingHost(Arc::clone(&host))),
        };
        let core = Core::new(
            Arc::new(FrameSlot::new()),
            self.settings,
            ports,
            self.cli,
            Lang::En,
        );
        Rig {
            core,
            replay,
            sources,
            host,
            stats: stats_log,
            store,
            system,
            now: Instant::now(),
        }
    }
}

impl Rig {
    /// A rig with nothing plugged in yet.
    pub fn builder() -> RigBuilder {
        RigBuilder {
            settings: AppSettings::default(),
            video: Vec::new(),
            audio: Vec::new(),
            cli: Cli::default(),
        }
    }

    /// The usual setup: a UGREEN capture card with its own audio input, and a
    /// webcam with its microphone.
    pub fn new() -> Self {
        Self::builder()
            .video(&[
                ("UGREEN 25173", "link:ugreen"),
                ("Integrated Camera", "link:webcam"),
            ])
            .audio(&[
                ("Microphone (Integrated Camera)", "audio:webcam"),
                ("HDMI (UGREEN 25173)", "audio:ugreen"),
            ])
            .build()
    }

    /// Moves the clock forward and lets the application take a turn.
    pub fn advance(&mut self, by: Duration) {
        self.now += by;
        self.core.tick_at(self.now, false);
    }

    /// The settings as they were last saved.
    pub fn saved(&self) -> Option<AppSettings> {
        self.store.last_saved()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_application_starts_on_the_first_card_with_its_own_audio() {
        let rig = Rig::new();
        let sources = rig.sources.lock().unwrap();
        assert_eq!(sources.last_capture().lock().unwrap().link, "link:ugreen");
        assert_eq!(
            sources.last_playback().lock().unwrap().device_id,
            "audio:ugreen"
        );
    }

    #[test]
    fn advancing_the_clock_lets_the_application_take_a_turn() {
        let mut rig = Rig::new();
        rig.advance(Duration::from_secs(2));
        assert!(rig.stats.lock().unwrap().len() <= 1);
        rig.advance(Duration::from_secs(2));
        assert!(!rig.stats.lock().unwrap().is_empty());
    }
}
