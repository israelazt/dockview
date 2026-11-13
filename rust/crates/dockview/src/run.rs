//! The composition root: reads the command line, finds the devices, creates the
//! real adapters, gives them to the application core and runs the event loop.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use dockview_app::cli::Cli;
use dockview_app::{Core, Ports};
use dockview_domain::frame::FrameSlot;
use dockview_domain::ports::StatsSink;
use dockview_domain::settings::{AppSettings, SettingsStore};
use dockview_platform::audio::enumerate_audio_devices;
use dockview_platform::mf::{enumerate_video_devices, MfSession};
use dockview_platform::perf::PlatformSystem;
use dockview_platform::replay_machine::PlatformReplay;
use dockview_platform::settings_store::FileSettingsStore;
use dockview_platform::source_machine::PlatformSources;
use dockview_platform::stats_log::StatsLog;
use dockview_platform::system_lang::detect_system_lang;
use winit::event_loop::{ControlFlow, EventLoop};

use crate::shell::helpers::on_helper_thread;
use crate::shell::host::{HostShared, ShellHost};
use crate::shell::Shell;

/// Reads the command line, finds the devices and runs the event loop.
pub fn run() {
    let slot = Arc::new(FrameSlot::new());
    let cli = match Cli::parse(std::env::args().skip(1)) {
        Ok(cli) => cli,
        Err(e) => {
            eprintln!("dockview: {e}");
            std::process::exit(2);
        }
    };

    let (video_devices, audio_devices) = if cli.test_pattern {
        (Vec::new(), Vec::new())
    } else {
        (
            on_helper_thread(|| {
                let session = MfSession::start().ok()?;
                enumerate_video_devices(&session).ok()
            }),
            on_helper_thread(|| enumerate_audio_devices().ok()),
        )
    };

    let event_loop = match EventLoop::new() {
        Ok(l) => l,
        Err(e) => {
            eprintln!("could not create event loop: {e}");
            return;
        }
    };
    event_loop.set_control_flow(ControlFlow::Poll);

    // The preview must look the same on every machine: ignore saved settings.
    let store = FileSettingsStore::at_default_location();
    let saved = if cli.ui_preview {
        AppSettings::default()
    } else {
        store.load()
    };
    let host = Rc::new(RefCell::new(HostShared::default()));
    let ports = Ports {
        settings: Box::new(store),
        replay: Box::new(PlatformReplay),
        sources: Box::new(PlatformSources),
        system: Arc::new(PlatformSystem::new()),
        stats_sink: cli
            .log_stats
            .as_deref()
            .and_then(|path| StatsLog::create(path).ok())
            .map(|log| Box::new(log) as Box<dyn StatsSink>),
        host: Box::new(ShellHost::new(Rc::clone(&host))),
    };
    let core = Core::new(
        slot,
        saved,
        ports,
        video_devices,
        audio_devices,
        cli,
        detect_system_lang(),
    );
    let mut shell = Shell::new(core, host);
    if let Err(e) = event_loop.run_app(&mut shell) {
        eprintln!("event loop error: {e}");
    }
}
