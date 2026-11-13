//! What the binary calls: the small surface of `Core`. Everything else in this
//! module tree is internal.

use super::*;

impl Core {
    /// The command line this run was started with.
    pub fn cli(&self) -> &Cli {
        &self.cli
    }

    /// Where the pictures to show (and to record) are published.
    pub fn output_slot(&self) -> &Arc<FrameSlot> {
        &self.slot
    }

    /// One turn of the event loop: keeps the sources alive, moves a scene
    /// change along, refreshes the lists while the sources popover is open
    /// (`sources_open`), measures, and runs the replay.
    pub fn tick(&mut self, sources_open: bool) {
        self.tick_at(Instant::now(), sources_open);
    }

    /// [`Core::tick`] at the time `now`, so tests can move the clock by hand.
    pub fn tick_at(&mut self, now: Instant, sources_open: bool) {
        self.refresh_devices();
        self.maintain_source(now);
        self.maintain_audio(now);
        self.maintain_overlays(now);
        self.tick_transition();
        if sources_open && self.sources.lists_are_stale(now, SOURCES_REFRESH) {
            self.refresh_sources();
        }
        self.update_stats(now);
        self.poll_replay();
        self.flush_settings(now);
    }

    /// What happened since the last call, to be shown to the user.
    pub fn take_events(&mut self) -> Vec<AppEvent> {
        std::mem::take(&mut self.events)
    }

    /// A new picture of `size` was uploaded to the screen.
    pub fn frame_uploaded(&mut self, size: (u32, u32)) {
        self.view.video_size = size;
        self.view.rendered += 1;
    }

    /// A picture took `ms` from arriving to being on screen.
    pub fn note_latency(&mut self, ms: f64) {
        self.measures.telemetry.note_latency(ms);
    }

    /// Pictures shown so far.
    pub fn rendered_frames(&self) -> u64 {
        self.view.rendered
    }

    /// Measurement windows closed so far (the first include start-up work).
    pub fn measurement_windows(&self) -> u32 {
        self.measures.telemetry.windows()
    }

    /// The language the interface is shown in.
    pub fn lang(&self) -> Lang {
        self.lang_mode().resolve(self.view.system_lang)
    }

    pub fn stats(&self) -> &Stats {
        &self.measures.stats
    }

    pub fn stats_open(&self) -> bool {
        self.view.stats_open
    }

    pub fn fullscreen(&self) -> bool {
        self.view.fullscreen
    }

    pub fn render_preset(&self) -> RenderPreset {
        self.settings.render_preset
    }

    /// Whether the last session ended in full screen.
    pub fn was_fullscreen(&self) -> bool {
        self.settings.was_fullscreen
    }

    /// Whether a recording is running, starting or waiting to start.
    pub fn rec_is_on(&self) -> bool {
        self.rec_view().is_on()
    }

    /// The system's light or dark setting, which `auto` follows.
    pub fn set_system_appearance(&mut self, appearance: Appearance) {
        self.view.system_appearance = appearance;
    }

    /// Hides the window from captures when any layer is a screen. Called once
    /// the window exists, and by itself whenever the sources change.
    pub fn apply_capture_exclusion(&mut self) {
        self.set_capture_exclusion(self.wants_exclusion());
    }

    /// Goes back to full screen if that is how the last session ended.
    pub fn restore_fullscreen(&mut self) {
        if self.settings.was_fullscreen {
            self.set_fullscreen(true);
        }
    }
}
