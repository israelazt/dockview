//! Building the app and starting it: the devices, the first source and the
//! event loop.

use super::*;

impl App {
    pub(super) fn new(
        slot: Arc<FrameSlot>,
        settings: AppSettings,
        ports: Ports,
        video_devices: Vec<VideoDevice>,
        audio_devices: Vec<AudioDevice>,
        cli: Cli,
    ) -> Self {
        let test_pattern = cli.test_pattern;
        let preview = cli.ui_preview;
        let show_stats = cli.show_stats;
        let stats_log = cli
            .log_stats
            .as_deref()
            .and_then(|path| stats_log::StatsLog::create(path).ok());
        // Screenshots must show the settled state, not a mid-transition frame.
        let animations = cli.screenshot.is_none() && system_animations_enabled();
        // The main source has a slot of its own that forwards to the output
        // one, so a scene can start or stop without restarting anything.
        let input_slot = Arc::new(FrameSlot::new());
        input_slot.set_forward(Some(Arc::clone(&slot)));
        let mut app = Self {
            slot,
            input_slot,
            scene: None,
            overlays: Vec::new(),
            scene_base: None,
            transition: None,
            source: Source::None,
            video_devices,
            audio_devices,
            video_index: None,
            audio_index: None,
            audio: None,
            settings,
            store: ports.settings,
            window: None,
            renderer: None,
            rendered: 0,
            video_size: (0, 0),
            fullscreen: false,
            cli,
            ui: Ui::new(),
            ui_gpu: None,
            ui_scale: 1.0,
            system_appearance: Appearance::Dark,
            system_lang: dockview_platform::system_lang::detect_system_lang(),
            exit_requested: false,
            controls: Controls::new(preview),
            pointer: Pointer::default(),
            last_frame: Instant::now(),
            needs_redraw: true,
            cursor_pointer: false,
            cursor_hidden: false,
            animations,
            stats_open: show_stats,
            stats_panel: StatsPanel::new(),
            stats: Stats::default(),
            stats_log,
            telemetry: Telemetry::new(),
            auto: AutoSwitch::new(),
            replay: ReplayService::new(ports.replay),
            toasts: ToastHost::new(),
            total_ram: perf::total_ram_bytes(),
            source_kind: SourceKind::Capture,
            loopback: None,
            machine: ports.sources,
            events: Vec::new(),
            monitors: Vec::new(),
            windows: Vec::new(),
            sources_refreshed: Instant::now(),
        };

        app.controls.set_animations(animations);

        if test_pattern {
            // The preview exists for screenshots, which must be the same every
            // time: one picture a second keeps the moving stripe at its start.
            let fps = if preview { 1 } else { 30 };
            app.source = Source::Test(
                app.machine
                    .start_test_pattern(Arc::clone(&app.input_slot), fps),
            );
            if preview {
                app.install_preview_devices();
            }
            app.open_requested_popover();
            app.start_cli_scene();
            app.finish_startup();
            return app;
        }

        // Restore the last devices, else the first video device and the audio
        // device whose name matches it.
        let video = pick_video(
            &app.video_devices,
            app.cli.video.as_deref(),
            app.settings.last_video_device_symbolic_link.as_deref(),
        );
        let matched_audio = app.match_audio(video);
        let audio = pick_audio(
            &app.audio_devices,
            app.settings.last_audio_device_id.as_deref(),
            matched_audio,
        );

        app.select_video(video);
        app.select_audio(audio);
        app.refresh_sources();
        app.restore_screen();
        let chose_main =
            app.cli.video.is_some() || app.cli.screen.is_some() || app.cli.capture_window.is_some();
        match startup_scene(
            app.cli.pip.is_some(),
            chose_main,
            app.cli.ui_preview,
            app.settings.active_scene,
        ) {
            StartupScene::FromCommandLine => app.start_cli_scene(),
            StartupScene::Saved(index) => app.activate_scene(Some(index)),
            StartupScene::None => {}
        }
        app.open_requested_popover();
        app.finish_startup();
        app
    }

    /// The last steps of starting: the hotkeys and the replay buffer, then the
    /// microphone and the recording if the command line asked for them.
    fn finish_startup(&mut self) {
        self.start_replay_services();
        if self.cli.mic {
            self.set_mic(true);
        }
        if self.cli.record {
            self.toggle_record();
        }
    }

    /// Opens the popover asked for on the command line (for screenshots).
    pub(super) fn open_requested_popover(&mut self) {
        if self.cli.open_settings {
            self.controls.toggle_settings();
        }
        if self.cli.open_replay {
            self.controls.toggle_replay();
        }
        if self.cli.open_sources {
            self.refresh_sources();
            self.controls.toggle_sources();
        }
        if self.cli.open_scenes {
            self.refresh_sources();
            self.controls.toggle_scenes();
        }
    }

    /// Fake devices so the interface can be explored without hardware.
    pub(super) fn install_preview_devices(&mut self) {
        let video = |name: &str, link: &str| VideoDevice {
            friendly_name: name.into(),
            symbolic_link: link.into(),
        };
        let audio = |name: &str, id: &str| AudioDevice {
            friendly_name: name.into(),
            device_id: id.into(),
        };
        self.video_devices = vec![
            video("UGREEN 25173", "preview:video:0"),
            video("Integrated Camera", "preview:video:1"),
        ];
        self.audio_devices = vec![
            audio("HDMI (UGREEN 25173)", "preview:audio:0"),
            audio("Microphone (Integrated Camera)", "preview:audio:1"),
            audio("Desktop Microphone (RØDE NT-USB Mini)", "preview:audio:2"),
        ];
        self.video_index = Some(0);
        self.audio_index = Some(0);
    }
}

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
    let test_pattern = cli.test_pattern;

    let (video_devices, audio_devices) = if test_pattern {
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
    let mut app = App::new(
        slot,
        saved,
        Ports {
            settings: Box::new(store),
            replay: Box::new(PlatformReplay),
            sources: Box::new(PlatformSources),
        },
        video_devices,
        audio_devices,
        cli,
    );
    if let Err(e) = event_loop.run_app(&mut app) {
        eprintln!("event loop error: {e}");
    }
}
