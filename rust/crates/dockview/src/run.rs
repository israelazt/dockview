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
use dockview_platform::perf::PlatformSystem;
use dockview_platform::replay_machine::PlatformReplay;
use dockview_platform::settings_store::FileSettingsStore;
use dockview_platform::source_machine::PlatformSources;
use dockview_platform::stats_log::StatsLog;
use dockview_platform::system_lang::detect_system_lang;
use winit::event_loop::{ControlFlow, EventLoop};

use crate::shell::host::{HostShared, ShellHost};
use crate::shell::present::{report_startup_failure, StartupFailure};
use crate::shell::Shell;

/// Reads the command line, finds the devices and runs the event loop.
pub fn run() {
    // A window program has no console; from a terminal, use that terminal's.
    dockview_platform::console::attach_parent_console();
    // The log comes first, so whatever goes wrong afterwards is written down.
    dockview_platform::log_file::install_default(true);
    dockview_domain::log_info!("Dockview {} starting", env!("CARGO_PKG_VERSION"));
    let slot = Arc::new(FrameSlot::new());
    let cli = match Cli::parse(std::env::args().skip(1)) {
        Ok(cli) => cli,
        Err(e) => {
            dockview_domain::log_error!("dockview: {e}");
            std::process::exit(2);
        }
    };

    let event_loop = match EventLoop::new() {
        Ok(l) => l,
        Err(e) => {
            dockview_domain::log_error!("could not create event loop: {e}");
            report_startup_failure(
                detect_system_lang(),
                StartupFailure::Window,
                &e.to_string(),
                cli.screenshot.is_some(),
            );
            std::process::exit(1);
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
        sources: Box::new(PlatformSources::new()),
        system: Arc::new(PlatformSystem::new()),
        stats_sink: cli
            .log_stats
            .as_deref()
            .and_then(|path| StatsLog::create(path).ok())
            .map(|log| Box::new(log) as Box<dyn StatsSink>),
        host: Box::new(ShellHost::new(Rc::clone(&host))),
    };
    let core = Core::new(slot, saved, ports, cli, detect_system_lang());
    let mut shell = Shell::new(core, host);
    if let Err(e) = event_loop.run_app(&mut shell) {
        dockview_domain::log_error!("event loop error: {e}");
    }
    if shell.failed() {
        std::process::exit(1);
    }
}
