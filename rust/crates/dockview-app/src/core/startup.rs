//! Building the core and starting it: the devices and the first source.

use super::*;

impl Core {
    pub fn new(
        slot: Arc<FrameSlot>,
        settings: AppSettings,
        ports: Ports,
        cli: Cli,
        system_lang: Lang,
    ) -> Self {
        let system = ports.system;
        // The synthetic source has no devices; the real machine lists them.
        let DeviceLists {
            video: video_devices,
            audio: audio_devices,
        } = if cli.test_pattern {
            DeviceLists::default()
        } else {
            ports.sources.devices()
        };
        let test_pattern = cli.test_pattern;
        let preview = cli.ui_preview;
        let show_stats = cli.show_stats;
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
            rendered: 0,
            video_size: (0, 0),
            fullscreen: false,
            cli,
            system_appearance: Appearance::Dark,
            host: ports.host,
            system_lang,
            stats_open: show_stats,
            stats: Stats::default(),
            stats_sink: ports.stats_sink,
            telemetry: Telemetry::new(Arc::clone(&system)),
            auto: AutoSwitch::new(),
            replay: ReplayService::new(ports.replay),
            total_ram: system.total_ram_bytes(),
            source_kind: SourceKind::Capture,
            loopback: None,
            machine: ports.sources,
            events: Vec::new(),
            monitors: Vec::new(),
            windows: Vec::new(),
            sources_refreshed: Instant::now(),
            audio_reconnect: Reconnect::new(),
            audio_lost: false,
        };

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
