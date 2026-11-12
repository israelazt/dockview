//! The live controls: a floating pill over the video and a settings popover.
//!
//! Immediate mode: every frame [`Controls::frame`] reads the pointer, decides
//! what is hovered/pressed using the *previous* frame's hit rectangles,
//! emits [`Action`]s, and draws. The application applies the actions.

use std::collections::HashMap;
use std::f32::consts::TAU;

use dockview_core::settings::{LangMode, RenderPreset, VideoModePref, MAX_AUDIO_OFFSET_MS};

use super::input::Pointer;
use super::stats::wrap_text;
use super::text::Weight;
use super::{Rect, TextStyle, Ui};
use crate::i18n::{tr, Key, Lang};
use crate::theme::{Color, Theme, ThemeMode};

/// Seconds of pointer inactivity before the controls fade out.
pub const IDLE_HIDE_SECS: f32 = 3.0;
/// Fade duration, in seconds.
pub const FADE_SECS: f32 = 0.2;
/// Audio sync change per click, in milliseconds.
pub const SYNC_STEP_MS: i32 = 10;
/// Volume change per scroll notch.
pub const WHEEL_VOLUME_STEP: f32 = 0.05;
/// Smoothing rate for hover fades (higher is faster; ~95% in 3/rate seconds).
const HOVER_RATE: f32 = 22.0;
/// Smoothing rate for the pressed state.
const PRESS_RATE: f32 = 34.0;
/// Smoothing rate for the sliding selection indicator.
const SLIDE_RATE: f32 = 20.0;
/// Time the popover takes to open or close, in seconds.
const POPOVER_SECS: f32 = 0.18;
/// How far the popover rises while opening, in logical pixels.
const POPOVER_RISE: f32 = 10.0;
/// Period of the "connecting" pulse, in seconds.
const PULSE_SECS: f32 = 1.2;
/// Width of the instant-replay popover.
const REPLAY_POPOVER_WIDTH: f32 = 324.0;
/// Most devices listed per kind in the popover.
const MAX_LISTED_DEVICES: usize = 5;

const PILL_HEIGHT: f32 = 56.0;
const BOTTOM_MARGIN: f32 = 28.0;
const PAD_X: f32 = 22.0;
const GAP: f32 = 18.0;
const BUTTON: f32 = 32.0;
const ICON_SIZE: f32 = 17.0;
const SLIDER_WIDTH: f32 = 96.0;

const POPOVER_WIDTH: f32 = 312.0;
const POPOVER_PAD: f32 = 16.0;
const POPOVER_GAP: f32 = 12.0;
const ROW_HEIGHT: f32 = 32.0;
const SECTION_GAP: f32 = 14.0;
const LABEL_HEIGHT: f32 = 20.0;

// Lucide code points.
const ICON_VOLUME_MUTE: char = '\u{e1ac}';
const ICON_VOLUME_LOW: char = '\u{e1aa}';
const ICON_VOLUME_HIGH: char = '\u{e1ab}';
const ICON_MINUS: char = '\u{e11c}';
const ICON_PLUS: char = '\u{e13d}';
const ICON_SETTINGS: char = '\u{e29a}';
const ICON_FULLSCREEN: char = '\u{e112}';
const ICON_EXIT_FULLSCREEN: char = '\u{e11a}';
const ICON_CHECK: char = '\u{e06c}';
const ICON_ACTIVITY: char = '\u{e038}';
const ICON_REPLAY: char = '\u{e1f5}';
const ICON_SAVE: char = '\u{e14d}';
const ICON_FOLDER: char = '\u{e247}';
const ICON_MIC: char = '\u{e118}';
const ICON_MIC_OFF: char = '\u{e119}';

const LABEL: TextStyle = TextStyle::new(13.0, Weight::Medium);
const BODY: TextStyle = TextStyle::new(13.0, Weight::Regular);
/// Height of the microphone row and of the line under it, in the replay popover.
const MIC_ROW: f32 = 30.0;
const MIC_METER: f32 = 14.0;
const SECTION: TextStyle = TextStyle::new(12.0, Weight::Medium);

const PRESETS: [RenderPreset; 3] = [
    RenderPreset::Performance,
    RenderPreset::Balanced,
    RenderPreset::Quality,
];
const THEMES: [ThemeMode; 3] = [ThemeMode::Auto, ThemeMode::Dark, ThemeMode::Light];
const LANGS: [LangMode; 3] = [LangMode::Auto, LangMode::En, LangMode::Es];
const VIDEO_MODES: [VideoModePref; 2] = [VideoModePref::Auto, VideoModePref::Compressed];

/// How healthy the video source is, for the status dot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    Live,
    Connecting,
    Problem,
    Idle,
}

/// State of the instant-replay buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayState {
    Off,
    Starting,
    Recording,
    /// This PC cannot record (no hardware encoder, unsupported format).
    Unavailable,
}

/// State of a whole-session recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecState {
    Idle,
    /// Asked to start; the file begins at the next keyframe.
    Starting,
    Recording,
}

/// What the recording controls display.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RecView {
    pub state: RecState,
    /// Footage written so far, in seconds.
    pub seconds: f32,
}

impl RecView {
    pub const fn idle() -> Self {
        Self {
            state: RecState::Idle,
            seconds: 0.0,
        }
    }

    pub fn is_on(&self) -> bool {
        self.state != RecState::Idle
    }
}

/// State of the microphone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicState {
    Off,
    Starting,
    Live,
    /// No microphone could be opened.
    Unavailable,
}

/// What the microphone controls display.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MicView {
    pub state: MicState,
    /// How loud it is right now, 0.0–1.0.
    pub level: f32,
}

impl MicView {
    pub const fn off() -> Self {
        Self {
            state: MicState::Off,
            level: 0.0,
        }
    }

    /// Turned on by the user (even if it could not start).
    pub fn is_on(&self) -> bool {
        self.state != MicState::Off
    }
}

/// What the replay controls display.
#[derive(Debug, Clone, PartialEq)]
pub struct ReplayView {
    pub state: ReplayState,
    /// A clip is being written right now.
    pub saving: bool,
    pub buffered_seconds: f32,
    pub buffered_mb: f32,
    /// Length of a clip, in seconds.
    pub keep_seconds: u32,
    /// Memory the buffer needs at this length, in bytes.
    pub memory_bytes: u64,
    /// The chosen length is the longest this PC's memory allows.
    pub at_limit: bool,
    pub folder: String,
    pub hotkey: String,
}

impl ReplayView {
    /// A turned-off replay with the given hotkey label.
    pub fn off(hotkey: &str) -> Self {
        Self {
            state: ReplayState::Off,
            saving: false,
            buffered_seconds: 0.0,
            buffered_mb: 0.0,
            keep_seconds: 30,
            memory_bytes: 0,
            at_limit: false,
            folder: String::new(),
            hotkey: hotkey.to_owned(),
        }
    }

    /// Whether the buffer is on (recording or starting up).
    pub fn is_on(&self) -> bool {
        matches!(self.state, ReplayState::Starting | ReplayState::Recording)
    }
}

/// Everything the controls display.
#[derive(Debug, Clone)]
pub struct Model {
    pub theme: Theme,
    pub lang: Lang,
    pub status: StatusKind,
    /// e.g. `1080p · 60 fps`; empty until the first frame arrives.
    pub resolution: String,
    pub volume: f32,
    pub sync_ms: i32,
    pub fullscreen: bool,
    pub video_devices: Vec<String>,
    pub video_selected: Option<usize>,
    pub audio_devices: Vec<String>,
    pub audio_selected: Option<usize>,
    /// The first entry is the Windows default microphone.
    pub mic_devices: Vec<String>,
    pub mic_selected: Option<usize>,
    pub preset: RenderPreset,
    pub theme_mode: ThemeMode,
    pub lang_mode: LangMode,
    pub video_mode: VideoModePref,
    pub stats_open: bool,
    pub replay: ReplayView,
    pub mic: MicView,
    pub rec: RecView,
}

/// A change the user asked for.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    SetVolume(f32),
    SetSync(i32),
    ToggleFullscreen,
    SelectVideo(usize),
    SelectAudio(usize),
    SelectMic(usize),
    ToggleMic,
    /// Start or stop recording the session.
    ToggleRecord,
    SetPreset(RenderPreset),
    SetTheme(ThemeMode),
    SetLang(LangMode),
    SetVideoMode(VideoModePref),
    ToggleStats,
    ToggleReplay,
    /// Move the clip length one step longer (+1) or shorter (-1).
    ReplayLength(i32),
    SaveReplay,
    OpenReplayFolder,
}

/// Identifies an interactive (or occluding) region.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Hit {
    Pill,
    Popover,
    Volume,
    SyncMinus,
    SyncPlus,
    Settings,
    Fullscreen,
    Video(usize),
    Audio(usize),
    MicDevice(usize),
    Preset(usize),
    Theme(usize),
    Lang(usize),
    VideoMode(usize),
    Stats,
    /// Pill button that opens the replay popover.
    Replay,
    /// Pill button that saves the replay.
    ReplaySave,
    /// Pill button that starts or stops a recording.
    Record,
    ReplayToggle,
    /// Only an animation key for the switch knob; never registered.
    ReplayKnob,
    ReplayShorter,
    ReplayLonger,
    ReplayPopSave,
    ReplayFolder,
    MicToggle,
    /// Only an animation key for the microphone switch knob; never registered.
    MicKnob,
}

impl Hit {
    /// Background regions only block clicks; they are not "buttons".
    fn is_widget(self) -> bool {
        !matches!(self, Hit::Pill | Hit::Popover)
    }

    /// Belongs to the settings popover.
    fn in_popover(self) -> bool {
        matches!(
            self,
            Hit::Popover
                | Hit::Video(_)
                | Hit::Audio(_)
                | Hit::MicDevice(_)
                | Hit::MicToggle
                | Hit::Preset(_)
                | Hit::Theme(_)
                | Hit::Lang(_)
                | Hit::VideoMode(_)
                | Hit::Stats
                | Hit::ReplayToggle
                | Hit::ReplayShorter
                | Hit::ReplayLonger
                | Hit::ReplayPopSave
                | Hit::ReplayFolder
        )
    }
}

/// Which popover is (or was last) open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PopoverKind {
    Settings,
    Replay,
}

pub struct Controls {
    settings_open: bool,
    replay_open: bool,
    popover_kind: PopoverKind,
    always_visible: bool,
    visibility: f32,
    idle: f32,
    last_pos: Option<(f32, f32)>,
    hovered: Option<Hit>,
    pressed: Option<Hit>,
    /// Rectangles registered by the previous frame; later entries are on top.
    hits: Vec<(Hit, Rect)>,
    next_hits: Vec<(Hit, Rect)>,
    volume_track: Rect,
    actions: Vec<Action>,
    animations: bool,
    /// Seconds since creation; drives the looping pulse.
    time: f32,
    /// Duration of the frame being processed.
    dt: f32,
    /// Smoothed (hover, pressed) amounts per widget, 0.0–1.0.
    amounts: HashMap<Hit, (f32, f32)>,
    /// Popover open/close progress, 0.0–1.0 (linear in time).
    popover_t: f32,
    /// Animated position of each segmented control's indicator.
    segments: Option<[f32; 4]>,
}

impl Controls {
    /// With `always_visible`, the controls never auto-hide (used by the
    /// interface preview).
    pub fn new(always_visible: bool) -> Self {
        Self {
            settings_open: false,
            replay_open: false,
            popover_kind: PopoverKind::Settings,
            always_visible,
            visibility: if always_visible { 1.0 } else { 0.0 },
            idle: IDLE_HIDE_SECS,
            last_pos: None,
            hovered: None,
            pressed: None,
            hits: Vec::new(),
            next_hits: Vec::new(),
            volume_track: Rect::new(0.0, 0.0, 1.0, 1.0),
            actions: Vec::new(),
            animations: true,
            time: 0.0,
            dt: 0.0,
            amounts: HashMap::new(),
            popover_t: 0.0,
            segments: None,
        }
    }

    /// Turns motion on or off (off snaps every transition, honouring the
    /// Windows "show animations" setting).
    pub fn set_animations(&mut self, on: bool) {
        self.animations = on;
    }

    pub fn take_actions(&mut self) -> Vec<Action> {
        std::mem::take(&mut self.actions)
    }

    /// Whether anything is currently drawn.
    pub fn is_visible(&self) -> bool {
        self.visibility > 0.01
    }

    /// Whether the pointer is over a clickable widget.
    pub fn wants_pointer(&self) -> bool {
        self.is_visible() && self.hovered.is_some_and(Hit::is_widget)
    }

    /// Closes the popover; returns whether it was open.
    pub fn close_settings(&mut self) -> bool {
        let was_open = self.popover_target();
        self.settings_open = false;
        self.replay_open = false;
        was_open
    }

    pub fn toggle_settings(&mut self) {
        self.settings_open = !self.settings_open;
        if self.settings_open {
            self.replay_open = false;
            self.popover_kind = PopoverKind::Settings;
        }
    }

    pub fn toggle_replay(&mut self) {
        self.replay_open = !self.replay_open;
        if self.replay_open {
            self.settings_open = false;
            self.popover_kind = PopoverKind::Replay;
        }
    }

    pub fn replay_open(&self) -> bool {
        self.replay_open
    }

    /// Whether a popover is meant to be showing (as opposed to fading out).
    fn popover_target(&self) -> bool {
        self.settings_open || self.replay_open
    }

    pub fn settings_open(&self) -> bool {
        self.settings_open
    }

    /// Runs one frame: input, then drawing.
    pub fn frame(&mut self, ui: &mut Ui, model: &Model, pointer: &Pointer, dt: f32) {
        self.dt = dt;
        self.time = (self.time + dt) % 3600.0;
        self.update_visibility(pointer, dt);
        if self.visibility <= 0.0 {
            self.hits.clear();
            self.hovered = None;
            self.pressed = None;
            return;
        }

        self.hovered = self.hit_at(pointer.pos);
        self.interact(model, pointer);
        // After input, so a click that opens the popover starts animating at once.
        self.update_popover_progress();

        ui.set_opacity(self.visibility);
        self.next_hits.clear();
        let pill = self.draw_pill(ui, model);
        if self.popover_t > 0.0 {
            let eased = ease_out_cubic(self.popover_t);
            ui.set_opacity(self.visibility * eased);
            ui.set_offset(0.0, (1.0 - eased) * POPOVER_RISE);
            match self.popover_kind {
                PopoverKind::Settings => self.draw_popover(ui, model, pill),
                PopoverKind::Replay => self.draw_replay_popover(ui, model, pill),
            }
            ui.set_offset(0.0, 0.0);
        }
        std::mem::swap(&mut self.hits, &mut self.next_hits);
        ui.set_opacity(1.0);
    }

    // ------------------------------------------------------------ visibility

    fn update_visibility(&mut self, pointer: &Pointer, dt: f32) {
        let moved = pointer.pos.is_some() && pointer.pos != self.last_pos;
        self.last_pos = pointer.pos;
        let over_ui = self.hit_at(pointer.pos).is_some();

        if moved || pointer.just_pressed || pointer.wheel != 0.0 {
            self.idle = 0.0;
        } else {
            self.idle += dt;
        }

        let keep = self.always_visible
            || self.idle < IDLE_HIDE_SECS
            || self.popover_target()
            || self.popover_t > 0.0
            || self.pressed.is_some()
            || over_ui;
        self.visibility = step_visibility(self.visibility, keep, dt);
    }

    // ----------------------------------------------------------------- input

    fn hit_at(&self, pos: Option<(f32, f32)>) -> Option<Hit> {
        let (x, y) = pos?;
        self.hits
            .iter()
            .rev()
            .find(|(_, rect)| rect.contains(x, y))
            .map(|(hit, _)| *hit)
    }

    fn interact(&mut self, model: &Model, pointer: &Pointer) {
        if pointer.just_pressed {
            self.pressed = self.hovered;
            match self.hovered {
                Some(Hit::Volume) => self.drag_volume(model, pointer),
                None if self.popover_target() => {
                    self.settings_open = false;
                    self.replay_open = false;
                }
                _ => {}
            }
        } else if pointer.down && self.pressed == Some(Hit::Volume) {
            self.drag_volume(model, pointer);
        }

        if pointer.just_released {
            if let (Some(pressed), Some(hovered)) = (self.pressed, self.hovered) {
                if pressed == hovered {
                    self.activate(pressed, model);
                }
            }
            self.pressed = None;
        }

        if pointer.wheel != 0.0 && self.hovered.is_some() {
            let volume = (model.volume + pointer.wheel * WHEEL_VOLUME_STEP).clamp(0.0, 1.0);
            self.actions.push(Action::SetVolume(round_percent(volume)));
        }
    }

    fn drag_volume(&mut self, model: &Model, pointer: &Pointer) {
        let Some((x, _)) = pointer.pos else {
            return;
        };
        let value =
            round_percent(((x - self.volume_track.x) / self.volume_track.w).clamp(0.0, 1.0));
        if (value - model.volume).abs() > f32::EPSILON {
            self.actions.push(Action::SetVolume(value));
        }
    }

    fn activate(&mut self, hit: Hit, model: &Model) {
        let sync =
            |delta: i32| (model.sync_ms + delta).clamp(-MAX_AUDIO_OFFSET_MS, MAX_AUDIO_OFFSET_MS);
        let action = match hit {
            Hit::Settings => {
                self.toggle_settings();
                return;
            }
            Hit::Replay => {
                self.toggle_replay();
                return;
            }
            Hit::ReplaySave | Hit::ReplayPopSave => Action::SaveReplay,
            Hit::Record => Action::ToggleRecord,
            Hit::ReplayToggle => Action::ToggleReplay,
            Hit::ReplayShorter => Action::ReplayLength(-1),
            Hit::ReplayLonger => Action::ReplayLength(1),
            Hit::ReplayFolder => Action::OpenReplayFolder,
            Hit::ReplayKnob | Hit::MicKnob => return,
            Hit::MicToggle => Action::ToggleMic,
            Hit::MicDevice(i) => Action::SelectMic(i),
            Hit::Fullscreen => Action::ToggleFullscreen,
            Hit::SyncMinus => Action::SetSync(sync(-SYNC_STEP_MS)),
            Hit::SyncPlus => Action::SetSync(sync(SYNC_STEP_MS)),
            Hit::Video(i) => Action::SelectVideo(i),
            Hit::Audio(i) => Action::SelectAudio(i),
            Hit::Preset(i) => Action::SetPreset(PRESETS[i]),
            Hit::Theme(i) => Action::SetTheme(THEMES[i]),
            Hit::Lang(i) => Action::SetLang(LANGS[i]),
            Hit::VideoMode(i) => Action::SetVideoMode(VIDEO_MODES[i]),
            Hit::Stats => Action::ToggleStats,
            Hit::Pill | Hit::Popover | Hit::Volume => return,
        };
        self.actions.push(action);
    }

    /// Advances the popover open/close progress and forgets its transient
    /// animation state once it is fully closed.
    fn update_popover_progress(&mut self) {
        let target = if self.popover_target() { 1.0 } else { 0.0 };
        self.popover_t = if self.animations {
            move_toward(self.popover_t, target, self.dt / POPOVER_SECS)
        } else {
            target
        };
        if self.popover_t <= 0.0 && !self.popover_target() {
            self.segments = None;
            self.amounts.retain(|hit, _| !hit.in_popover());
        }
    }

    /// Eases `current` towards `target`; snaps when animations are off.
    fn approach(&self, current: f32, target: f32, rate: f32) -> f32 {
        if self.animations {
            smooth(current, target, self.dt, rate)
        } else {
            target
        }
    }

    /// Updates and returns the smoothed (hover, pressed) amounts of `hit`.
    fn animate(&mut self, hit: Hit, hover: bool, press: bool) -> (f32, f32) {
        let (h, p) = self.amounts.get(&hit).copied().unwrap_or((0.0, 0.0));
        let h = self.approach(h, if hover { 1.0 } else { 0.0 }, HOVER_RATE);
        let p = self.approach(p, if press { 1.0 } else { 0.0 }, PRESS_RATE);
        self.amounts.insert(hit, (h, p));
        (h, p)
    }

    fn amounts(&mut self, hit: Hit) -> (f32, f32) {
        let (hover, press) = (self.is_hovered(hit), self.is_pressed(hit));
        self.animate(hit, hover, press)
    }

    fn is_hovered(&self, hit: Hit) -> bool {
        self.hovered == Some(hit)
    }

    fn is_pressed(&self, hit: Hit) -> bool {
        self.pressed == Some(hit) && self.hovered == Some(hit)
    }

    fn register(&mut self, hit: Hit, rect: Rect) {
        // A popover that is fading out must not receive clicks.
        if hit.in_popover() && !self.popover_target() {
            return;
        }
        self.next_hits.push((hit, rect));
    }

    // --------------------------------------------------------------- drawing

    #[allow(clippy::too_many_arguments)]
    fn icon_button(
        &mut self,
        ui: &mut Ui,
        m: &Model,
        hit: Hit,
        icon: char,
        cx: f32,
        cy: f32,
        size: f32,
        active: bool,
    ) {
        let rect = Rect::new(cx - BUTTON / 2.0, cy - BUTTON / 2.0, BUTTON, BUTTON);
        self.register(hit, rect);
        let (hover, press) = self.amounts(hit);
        let strength = hover.max(if active { 1.0 } else { 0.0 });
        let alpha = m.theme.hover.a * (strength + 0.9 * press);
        if alpha > 0.002 {
            ui.rect(rect, BUTTON / 2.0, m.theme.hover.with_alpha(alpha.min(1.0)));
        }
        // The icon dips slightly under the finger.
        ui.icon(icon, cx, cy, size * (1.0 - 0.1 * press), m.theme.text);
    }

    fn draw_pill(&mut self, ui: &mut Ui, m: &Model) -> Rect {
        let t = &m.theme;
        let value = BODY.tabular();

        let (status_key, dot) = match m.status {
            StatusKind::Live => (Key::Live, t.live),
            StatusKind::Connecting => (Key::Connecting, t.warn),
            StatusKind::Problem => (Key::Disconnected, t.error),
            StatusKind::Idle => (Key::NoDevice, t.text_dim),
        };
        let status = tr(m.lang, status_key);
        let sync_label = tr(m.lang, Key::Sync);
        let percent = format!("{:.0}%", m.volume * 100.0);
        let sync_value = format!("{:+} ms", m.sync_ms).replace('-', "−");
        let volume_icon = if m.volume <= 0.0 {
            ICON_VOLUME_MUTE
        } else if m.volume < 0.5 {
            ICON_VOLUME_LOW
        } else {
            ICON_VOLUME_HIGH
        };

        // Widths use the widest plausible value so the pill does not resize
        // while numbers change.
        let status_w = ui.measure(status, LABEL).0;
        let res_w = ui
            .measure(&m.resolution, value)
            .0
            .max(ui.measure("1080p · 60 fps", value).0);
        let sync_label_w = ui.measure(sync_label, BODY).0;
        let percent_w = ui.measure("100%", value).0;
        let sync_value_w = ui.measure("−300 ms", value).0;

        let status_block = 8.0 + 8.0 + status_w + 12.0 + res_w;
        let volume_block = ICON_SIZE + 10.0 + SLIDER_WIDTH + 10.0 + percent_w;
        let sync_block = sync_label_w + 10.0 + BUTTON + sync_value_w + BUTTON;
        let buttons_block = BUTTON * 2.0 + 4.0;
        let divider = GAP * 2.0 + 1.0;
        let replay_block = BUTTON * 3.0 + 8.0 + divider;
        let width = PAD_X * 2.0
            + status_block
            + volume_block
            + sync_block
            + replay_block
            + buttons_block
            + divider * 3.0;

        let (screen_w, screen_h) = ui.logical_size();
        let pill = Rect::new(
            ((screen_w - width) / 2.0).round(),
            (screen_h - BOTTOM_MARGIN - PILL_HEIGHT).round(),
            width.round(),
            PILL_HEIGHT,
        );
        self.register(Hit::Pill, pill);
        let cy = pill.y + PILL_HEIGHT / 2.0;

        ui.shadow(pill, PILL_HEIGHT / 2.0, 22.0, t.shadow);
        ui.panel(pill, PILL_HEIGHT / 2.0, t.surface, t.border, 1.0);

        let line_h = ui.measure("Ag", BODY).1;
        let text_y = cy - line_h / 2.0;
        let mut x = pill.x + PAD_X;

        // Status. While connecting, the dot pulses gently.
        let pulse = if m.status == StatusKind::Connecting && self.animations {
            0.55 + 0.45 * (0.5 + 0.5 * (self.time * TAU / PULSE_SECS).sin())
        } else {
            1.0
        };
        ui.rect(
            Rect::new(x, cy - 4.0, 8.0, 8.0),
            4.0,
            dot.with_alpha(dot.a * pulse),
        );
        x += 16.0;
        ui.text(status, x, text_y, LABEL, t.text);
        x += status_w + 12.0;
        if !m.resolution.is_empty() {
            ui.text(&m.resolution, x, text_y, value, t.text_dim);
        }
        x += res_w;
        x = divider_at(ui, t, x, cy);

        // Volume.
        ui.icon(volume_icon, x + ICON_SIZE / 2.0, cy, ICON_SIZE, t.text);
        x += ICON_SIZE + 10.0;
        self.draw_slider(ui, m, Rect::new(x, cy - 6.0, SLIDER_WIDTH, 12.0));
        x += SLIDER_WIDTH + 10.0;
        ui.text(&percent, x, text_y, value, t.text_dim);
        x += percent_w;
        x = divider_at(ui, t, x, cy);

        // Sync.
        ui.text(sync_label, x, text_y, BODY, t.text_dim);
        x += sync_label_w + 10.0;
        self.icon_button(
            ui,
            m,
            Hit::SyncMinus,
            ICON_MINUS,
            x + BUTTON / 2.0,
            cy,
            14.0,
            false,
        );
        x += BUTTON;
        let value_x = x + (sync_value_w - ui.measure(&sync_value, value).0) / 2.0;
        ui.text(&sync_value, value_x, text_y, value, t.text);
        x += sync_value_w;
        self.icon_button(
            ui,
            m,
            Hit::SyncPlus,
            ICON_PLUS,
            x + BUTTON / 2.0,
            cy,
            14.0,
            false,
        );
        x += BUTTON;
        x = divider_at(ui, t, x, cy);

        // Instant replay: its options, and "save the last N".
        let replay_open = self.replay_open;
        self.icon_button(
            ui,
            m,
            Hit::Replay,
            ICON_REPLAY,
            x + BUTTON / 2.0,
            cy,
            ICON_SIZE,
            replay_open,
        );
        if m.replay.is_on() {
            // A small red dot while the buffer is running.
            let pulse = if m.replay.state == ReplayState::Starting && self.animations {
                0.5 + 0.5 * (self.time * TAU / PULSE_SECS).sin().abs()
            } else {
                1.0
            };
            ui.rect(
                Rect::new(x + BUTTON / 2.0 + 3.0, cy - 12.0, 9.0, 9.0),
                4.5,
                t.error.with_alpha(pulse),
            );
        }
        if m.mic.state == MicState::Live {
            // The microphone is open: a red microphone at the button's corner.
            ui.icon(ICON_MIC, x + BUTTON / 2.0 + 10.0, cy + 10.0, 11.0, t.error);
        }
        x += BUTTON + 4.0;
        self.save_button(ui, m, x + BUTTON / 2.0, cy);
        x += BUTTON + 4.0;
        self.record_button(ui, m, x + BUTTON / 2.0, cy);
        x += BUTTON;
        x = divider_at(ui, t, x, cy);

        // Actions.
        let open = self.settings_open;
        self.icon_button(
            ui,
            m,
            Hit::Settings,
            ICON_SETTINGS,
            x + BUTTON / 2.0,
            cy,
            ICON_SIZE,
            open,
        );
        x += BUTTON + 4.0;
        let fullscreen_icon = if m.fullscreen {
            ICON_EXIT_FULLSCREEN
        } else {
            ICON_FULLSCREEN
        };
        self.icon_button(
            ui,
            m,
            Hit::Fullscreen,
            fullscreen_icon,
            x + BUTTON / 2.0,
            cy,
            ICON_SIZE,
            false,
        );

        pill
    }

    /// The pill's "save clip" button: dimmed while the buffer is off, and
    /// pulsing while a clip is being written.
    fn save_button(&mut self, ui: &mut Ui, m: &Model, cx: f32, cy: f32) {
        let t = &m.theme;
        let rect = Rect::new(cx - BUTTON / 2.0, cy - BUTTON / 2.0, BUTTON, BUTTON);
        self.register(Hit::ReplaySave, rect);
        let (hover, press) = self.amounts(Hit::ReplaySave);
        let alpha = t.hover.a * (hover + 0.9 * press);
        if alpha > 0.002 {
            ui.rect(rect, BUTTON / 2.0, t.hover.with_alpha(alpha.min(1.0)));
        }
        let color = if m.replay.is_on() { t.text } else { t.text_dim };
        let pulse = if m.replay.saving && self.animations {
            0.45 + 0.55 * (0.5 + 0.5 * (self.time * TAU / 0.8).sin())
        } else {
            1.0
        };
        ui.icon(
            ICON_SAVE,
            cx,
            cy,
            ICON_SIZE * (1.0 - 0.1 * press),
            color.with_alpha(color.a * pulse),
        );
    }

    /// The pill's record button: a red circle when idle, a red square while recording.
    fn record_button(&mut self, ui: &mut Ui, m: &Model, cx: f32, cy: f32) {
        let t = &m.theme;
        let rect = Rect::new(cx - BUTTON / 2.0, cy - BUTTON / 2.0, BUTTON, BUTTON);
        self.register(Hit::Record, rect);
        let (hover, press) = self.amounts(Hit::Record);
        let alpha = t.hover.a * (hover + 0.9 * press);
        if alpha > 0.002 {
            ui.rect(rect, BUTTON / 2.0, t.hover.with_alpha(alpha.min(1.0)));
        }
        let scale = 1.0 - 0.1 * press;
        match m.rec.state {
            RecState::Idle => {
                let size = 14.0 * scale;
                ui.rect(
                    Rect::new(cx - size / 2.0, cy - size / 2.0, size, size),
                    size / 2.0,
                    t.error,
                );
            }
            RecState::Starting | RecState::Recording => {
                let pulse = if m.rec.state == RecState::Starting && self.animations {
                    0.5 + 0.5 * (self.time * TAU / PULSE_SECS).sin().abs()
                } else {
                    1.0
                };
                let size = 13.0 * scale;
                ui.rect(
                    Rect::new(cx - size / 2.0, cy - size / 2.0, size, size),
                    3.0,
                    t.error.with_alpha(pulse),
                );
            }
        }
    }

    /// An on/off switch. The knob glides when `on` changes.
    fn switch(&mut self, ui: &mut Ui, m: &Model, track: Rect, on: bool, knob: Hit) {
        let t = &m.theme;
        let (amount, _) = self.animate(knob, on, false);
        let fill = lerp_color(t.track, t.accent, amount);
        ui.rect(track, track.h / 2.0, fill);
        let r = track.h / 2.0 - 2.0;
        let knob_x = track.x + 2.0 + (track.w - track.h) * amount;
        ui.rect(
            Rect::new(knob_x, track.y + 2.0, r * 2.0, r * 2.0),
            r,
            Color::hex(0xFFFFFF, 1.0),
        );
    }

    /// The replay popover: switch, clip length, save and folder.
    fn draw_replay_popover(&mut self, ui: &mut Ui, m: &Model, pill: Rect) {
        let t = &m.theme;
        let r = &m.replay;
        let lang = m.lang;
        let width = REPLAY_POPOVER_WIDTH;
        let inner_w = width - POPOVER_PAD * 2.0;
        let small = TextStyle::new(12.5, Weight::Regular);
        let hint_style = TextStyle::new(12.0, Weight::Regular);

        let status = match r.state {
            ReplayState::Off => tr(lang, Key::ReplayOff).to_owned(),
            ReplayState::Starting => tr(lang, Key::ReplayStarting).to_owned(),
            ReplayState::Unavailable => tr(lang, Key::ReplayUnavailable).to_owned(),
            ReplayState::Recording => format!(
                "{} · {} · {:.0} MB",
                tr(lang, Key::ReplayRecording),
                crate::i18n::duration_label(r.buffered_seconds.floor() as u32),
                r.buffered_mb
            ),
        };
        let status_lines = wrap_text(ui, &status, small, inner_w);
        let memory_key = if r.at_limit {
            Key::ReplayMemoryLimit
        } else {
            Key::ReplayMemory
        };
        let hint = format!(
            "{} {}",
            tr(lang, memory_key).replace("{size}", &crate::i18n::size_label(r.memory_bytes)),
            tr(lang, Key::ReplayHint).replace("{hotkey}", &r.hotkey)
        );
        let hint_lines = wrap_text(ui, &hint, hint_style, inner_w);

        let title_h = 30.0;
        let status_h = status_lines.len() as f32 * 18.0;
        let hint_h = hint_lines.len() as f32 * 16.0;
        let stepper_h = 40.0;
        let button_h = 40.0;
        let folder_h = 32.0;
        let height = POPOVER_PAD * 2.0
            + title_h
            + 4.0
            + status_h
            + 14.0
            + LABEL_HEIGHT
            + stepper_h
            + 8.0
            + hint_h
            + 14.0
            + MIC_ROW
            + 4.0
            + MIC_METER
            + 14.0
            + button_h
            + 8.0
            + folder_h;

        let (screen_w, _) = ui.logical_size();
        let anchor = self
            .next_hits
            .iter()
            .find(|(h, _)| *h == Hit::Replay)
            .map_or(pill.x + pill.w / 2.0, |(_, rect)| rect.center().0);
        let x = (anchor - width / 2.0)
            .min(screen_w - width - POPOVER_GAP)
            .max(POPOVER_GAP)
            .round();
        let top = (pill.y - POPOVER_GAP - height).max(POPOVER_GAP).round();
        let rect = Rect::new(x, top, width, height);
        self.register(Hit::Popover, rect);
        ui.shadow(rect, 20.0, 26.0, t.shadow);
        ui.panel(rect, 20.0, t.surface, t.border, 1.0);

        let inner_x = x + POPOVER_PAD;
        let mut y = top + POPOVER_PAD;
        let line_h = ui.measure("Ag", BODY).1;

        // Title row: the whole row toggles the buffer.
        let row = Rect::new(inner_x, y, inner_w, title_h);
        self.register(Hit::ReplayToggle, row);
        let (hover, press) = self.amounts(Hit::ReplayToggle);
        let shade = t.hover.a * (hover * 0.6 + 0.9 * press);
        if shade > 0.002 {
            ui.rect(
                Rect::new(row.x - 6.0, row.y - 2.0, row.w + 12.0, row.h + 4.0),
                10.0,
                t.hover.with_alpha(shade.min(1.0)),
            );
        }
        let cy = y + title_h / 2.0;
        let icon_color = if r.is_on() { t.error } else { t.text_dim };
        ui.icon(ICON_REPLAY, inner_x + 10.0, cy, 17.0, icon_color);
        ui.text(
            tr(lang, Key::InstantReplay),
            inner_x + 30.0,
            cy - line_h / 2.0,
            TextStyle::new(14.0, Weight::SemiBold),
            t.text,
        );
        self.switch(
            ui,
            m,
            Rect::new(inner_x + inner_w - 42.0, cy - 11.0, 42.0, 22.0),
            r.is_on(),
            Hit::ReplayKnob,
        );
        y += title_h + 4.0;

        for line in &status_lines {
            ui.text(line, inner_x, y, small, t.text_dim);
            y += 18.0;
        }
        y += 14.0;

        // Clip length: [ − ]  value  [ + ]
        ui.text(tr(lang, Key::ReplayKeep), inner_x, y, SECTION, t.text_dim);
        y += LABEL_HEIGHT;
        let track = Rect::new(inner_x, y, inner_w, stepper_h);
        ui.rect(track, 12.0, t.segment_track);
        let step_hits = [
            (Hit::ReplayShorter, ICON_MINUS, track.x + 4.0),
            (
                Hit::ReplayLonger,
                ICON_PLUS,
                track.x + track.w - 4.0 - BUTTON,
            ),
        ];
        for (hit, icon, bx) in step_hits {
            let button = Rect::new(bx, y + (stepper_h - BUTTON) / 2.0, BUTTON, BUTTON);
            self.register(hit, button);
            let (hover, press) = self.amounts(hit);
            let alpha = t.hover.a * (hover + 0.9 * press);
            if alpha > 0.002 {
                ui.rect(button, 10.0, t.hover.with_alpha(alpha.min(1.0)));
            }
            let (cx, cy) = button.center();
            ui.icon(icon, cx, cy, 15.0 * (1.0 - 0.1 * press), t.text);
        }
        let value = crate::i18n::duration_label(r.keep_seconds);
        let value_style = TextStyle::new(15.0, Weight::Medium).tabular();
        let value_w = ui.measure(&value, value_style).0;
        let value_h = ui.measure("Ag", value_style).1;
        ui.text(
            &value,
            track.x + (track.w - value_w) / 2.0,
            y + (stepper_h - value_h) / 2.0,
            value_style,
            t.text,
        );
        y += stepper_h + 8.0;

        for line in &hint_lines {
            ui.text(line, inner_x, y, hint_style, t.text_dim);
            y += 16.0;
        }
        y += 14.0;

        // Microphone: switch, and a level meter while it is open.
        let mic_row = Rect::new(inner_x, y, inner_w, MIC_ROW);
        self.register(Hit::MicToggle, mic_row);
        let (hover, press) = self.amounts(Hit::MicToggle);
        let shade = t.hover.a * (hover * 0.6 + 0.9 * press);
        if shade > 0.002 {
            ui.rect(
                Rect::new(
                    mic_row.x - 6.0,
                    mic_row.y - 2.0,
                    mic_row.w + 12.0,
                    mic_row.h + 4.0,
                ),
                10.0,
                t.hover.with_alpha(shade.min(1.0)),
            );
        }
        let mic_cy = y + MIC_ROW / 2.0;
        let (mic_icon, mic_color) = match m.mic.state {
            MicState::Live | MicState::Starting => (ICON_MIC, t.error),
            MicState::Unavailable => (ICON_MIC_OFF, t.text_dim),
            MicState::Off => (ICON_MIC_OFF, t.text_dim),
        };
        ui.icon(mic_icon, inner_x + 10.0, mic_cy, 17.0, mic_color);
        ui.text(
            tr(lang, Key::Microphone),
            inner_x + 30.0,
            mic_cy - line_h / 2.0,
            TextStyle::new(14.0, Weight::SemiBold),
            t.text,
        );
        self.switch(
            ui,
            m,
            Rect::new(inner_x + inner_w - 42.0, mic_cy - 11.0, 42.0, 22.0),
            m.mic.is_on(),
            Hit::MicKnob,
        );
        y += MIC_ROW + 4.0;
        match m.mic.state {
            MicState::Unavailable => {
                ui.text(
                    tr(lang, Key::MicUnavailable),
                    inner_x,
                    y,
                    hint_style,
                    t.error,
                );
            }
            MicState::Off => {
                ui.text(tr(lang, Key::MicHint), inner_x, y, hint_style, t.text_dim);
            }
            MicState::Starting | MicState::Live => {
                let bar = Rect::new(inner_x, y + (MIC_METER - 4.0) / 2.0, inner_w, 4.0);
                ui.rect(bar, 2.0, t.track);
                // Loudness feels linear on a square-root scale.
                let filled = m.mic.level.clamp(0.0, 1.0).sqrt() * bar.w;
                if filled >= 1.0 {
                    ui.rect(Rect::new(bar.x, bar.y, filled, bar.h), 2.0, t.accent);
                }
            }
        }
        y += MIC_METER + 14.0;

        // Save button.
        let button = Rect::new(inner_x, y, inner_w, button_h);
        self.register(Hit::ReplayPopSave, button);
        let (hover, press) = self.amounts(Hit::ReplayPopSave);
        let enabled = r.is_on();
        let (fill, label_color) = if enabled {
            let on_accent = match t.appearance {
                crate::theme::Appearance::Dark => Color::hex(0x04141F, 1.0),
                crate::theme::Appearance::Light => Color::hex(0xFFFFFF, 1.0),
            };
            (t.accent, on_accent)
        } else {
            (t.segment_track, t.text_dim)
        };
        ui.rect(button, 12.0, fill);
        let sheen = Color::hex(0xFFFFFF, 0.0).with_alpha(0.14 * hover + 0.14 * press);
        if enabled && sheen.a > 0.002 {
            ui.rect(button, 12.0, sheen);
        }
        let label = if r.saving {
            tr(lang, Key::ToastSaving)
        } else {
            tr(lang, Key::SaveClip)
        };
        let label_style = TextStyle::new(13.5, Weight::SemiBold);
        let label_w = ui.measure(label, label_style).0 + 26.0;
        let label_h = ui.measure("Ag", label_style).1;
        let start = button.x + (button.w - label_w) / 2.0;
        ui.icon(
            ICON_SAVE,
            start + 8.0,
            y + button_h / 2.0,
            16.0,
            label_color,
        );
        ui.text(
            label,
            start + 26.0,
            y + (button_h - label_h) / 2.0,
            label_style,
            label_color,
        );
        y += button_h + 8.0;

        // Folder.
        let row = Rect::new(inner_x, y, inner_w, folder_h);
        self.register(Hit::ReplayFolder, row);
        let (hover, press) = self.amounts(Hit::ReplayFolder);
        let alpha = t.hover.a * (hover + 0.9 * press);
        if alpha > 0.002 {
            ui.rect(row, 9.0, t.hover.with_alpha(alpha.min(1.0)));
        }
        let cy = y + folder_h / 2.0;
        ui.icon(ICON_FOLDER, inner_x + 18.0, cy, 15.0, t.text_dim);
        let open_label = tr(lang, Key::OpenFolder);
        ui.text(open_label, inner_x + 36.0, cy - line_h / 2.0, BODY, t.text);
        let used = 36.0 + ui.measure(open_label, BODY).0 + 12.0;
        if inner_w - used > 40.0 {
            let path = fit_text(ui, &r.folder, hint_style, inner_w - used - 8.0);
            let path_w = ui.measure(&path, hint_style).0;
            let path_h = ui.measure("Ag", hint_style).1;
            ui.text(
                &path,
                inner_x + inner_w - path_w - 8.0,
                cy - path_h / 2.0,
                hint_style,
                t.text_dim,
            );
        }
    }

    fn draw_slider(&mut self, ui: &mut Ui, m: &Model, bounds: Rect) {
        let t = &m.theme;
        let (_, cy) = bounds.center();
        let track = Rect::new(bounds.x, cy - 2.0, bounds.w, 4.0);
        self.volume_track = track;
        self.register(
            Hit::Volume,
            Rect::new(track.x - 8.0, cy - 12.0, track.w + 16.0, 24.0),
        );

        let value = m.volume.clamp(0.0, 1.0);
        ui.rect(track, 2.0, t.track);
        ui.rect(
            Rect::new(track.x, track.y, track.w * value, track.h),
            2.0,
            t.accent,
        );
        let dragging = self.pressed == Some(Hit::Volume);
        let (grow, _) = self.animate(
            Hit::Volume,
            self.is_hovered(Hit::Volume) || dragging,
            dragging,
        );
        let knob_x = track.x + track.w * value;
        // A soft halo and a slightly larger knob while the slider is in use.
        let halo = 6.0 + 6.0 * grow;
        if grow > 0.01 {
            ui.rect(
                Rect::new(knob_x - halo, cy - halo, halo * 2.0, halo * 2.0),
                halo,
                t.accent.with_alpha(0.22 * grow),
            );
        }
        let r = 6.0 + 1.0 * grow;
        ui.rect(Rect::new(knob_x - r, cy - r, r * 2.0, r * 2.0), r, t.text);
    }

    fn draw_popover(&mut self, ui: &mut Ui, m: &Model, pill: Rect) {
        let t = &m.theme;
        let height = popover_height(m);
        let (screen_w, _) = ui.logical_size();
        let x = (pill.x + pill.w - POPOVER_WIDTH)
            .min(screen_w - POPOVER_WIDTH - POPOVER_GAP)
            .max(POPOVER_GAP)
            .round();
        let y = (pill.y - POPOVER_GAP - height).max(POPOVER_GAP).round();
        let rect = Rect::new(x, y, POPOVER_WIDTH, height);
        self.register(Hit::Popover, rect);

        ui.shadow(rect, 20.0, 26.0, t.shadow);
        ui.panel(rect, 20.0, t.surface, t.border, 1.0);

        let inner_x = x + POPOVER_PAD;
        let inner_w = POPOVER_WIDTH - POPOVER_PAD * 2.0;
        let mut y = y + POPOVER_PAD;

        // Indicator positions: start at the current choice, then slide.
        let targets = [
            PRESETS.iter().position(|v| *v == m.preset).unwrap_or(1) as f32,
            THEMES.iter().position(|v| *v == m.theme_mode).unwrap_or(0) as f32,
            LANGS.iter().position(|v| *v == m.lang_mode).unwrap_or(0) as f32,
            VIDEO_MODES
                .iter()
                .position(|v| *v == m.video_mode)
                .unwrap_or(0) as f32,
        ];
        let mut slide = self.segments.unwrap_or(targets);
        for (position, target) in slide.iter_mut().zip(targets) {
            *position = self.approach(*position, target, SLIDE_RATE);
        }
        self.segments = Some(slide);

        y = self.device_section(
            ui,
            m,
            Key::Video,
            &m.video_devices,
            m.video_selected,
            Hit::Video,
            inner_x,
            y,
            inner_w,
        );
        y += SECTION_GAP;
        y = self.device_section(
            ui,
            m,
            Key::Audio,
            &m.audio_devices,
            m.audio_selected,
            Hit::Audio,
            inner_x,
            y,
            inner_w,
        );
        y += SECTION_GAP;
        y = self.device_section(
            ui,
            m,
            Key::Microphone,
            &m.mic_devices,
            m.mic_selected,
            Hit::MicDevice,
            inner_x,
            y,
            inner_w,
        );
        y += SECTION_GAP;

        let lang = m.lang;
        let modes = [tr(lang, Key::Auto), tr(lang, Key::ModeCompressed)];
        let mode_selected = VIDEO_MODES
            .iter()
            .position(|v| *v == m.video_mode)
            .unwrap_or(0);
        y = self.segmented_section(
            ui,
            m,
            Key::VideoMode,
            &modes,
            mode_selected,
            slide[3],
            Hit::VideoMode,
            inner_x,
            y,
            inner_w,
        );
        y += SECTION_GAP;
        let presets = [
            tr(lang, Key::PresetPerformance),
            tr(lang, Key::PresetBalanced),
            tr(lang, Key::PresetHigh),
        ];
        let preset_selected = PRESETS.iter().position(|p| *p == m.preset).unwrap_or(1);
        y = self.segmented_section(
            ui,
            m,
            Key::ImageQuality,
            &presets,
            preset_selected,
            slide[0],
            Hit::Preset,
            inner_x,
            y,
            inner_w,
        );
        y += SECTION_GAP;

        let themes = [
            tr(lang, Key::Auto),
            tr(lang, Key::Dark),
            tr(lang, Key::Light),
        ];
        let theme_selected = THEMES.iter().position(|v| *v == m.theme_mode).unwrap_or(0);
        y = self.segmented_section(
            ui,
            m,
            Key::Theme,
            &themes,
            theme_selected,
            slide[1],
            Hit::Theme,
            inner_x,
            y,
            inner_w,
        );
        y += SECTION_GAP;

        let langs = [
            tr(lang, Key::Auto),
            tr(lang, Key::English),
            tr(lang, Key::Spanish),
        ];
        let lang_selected = LANGS.iter().position(|v| *v == m.lang_mode).unwrap_or(0);
        y = self.segmented_section(
            ui,
            m,
            Key::Language,
            &langs,
            lang_selected,
            slide[2],
            Hit::Lang,
            inner_x,
            y,
            inner_w,
        );
        y += SECTION_GAP;
        self.stats_row(ui, m, inner_x, y, inner_w);
    }

    /// The row that toggles the statistics panel.
    fn stats_row(&mut self, ui: &mut Ui, m: &Model, x: f32, y: f32, w: f32) {
        let t = &m.theme;
        let row = Rect::new(x, y, w, ROW_HEIGHT);
        self.register(Hit::Stats, row);
        let (hover, press) = self.amounts(Hit::Stats);
        let alpha = t.hover.a * (hover + 0.9 * press);
        if alpha > 0.002 {
            ui.rect(row, 9.0, t.hover.with_alpha(alpha.min(1.0)));
        }
        let cy = y + ROW_HEIGHT / 2.0;
        ui.icon(ICON_ACTIVITY, x + 20.0, cy, 15.0, t.text_dim);
        let line_h = ui.measure("Ag", BODY).1;
        ui.text(
            tr(m.lang, Key::Statistics),
            x + 38.0,
            cy - line_h / 2.0,
            BODY,
            t.text,
        );
        if m.stats_open {
            ui.icon(ICON_CHECK, x + w - 18.0, cy, 15.0, t.accent);
        } else {
            let hint = TextStyle::new(12.0, Weight::Medium);
            let key_w = ui.measure("I", hint).0 + 12.0;
            let key = Rect::new(x + w - 10.0 - key_w, cy - 10.0, key_w, 20.0);
            ui.rect(key, 6.0, t.segment_track);
            ui.text("I", key.x + 6.0, cy - 9.0, hint, t.text_dim);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn device_section(
        &mut self,
        ui: &mut Ui,
        m: &Model,
        title: Key,
        devices: &[String],
        selected: Option<usize>,
        hit: fn(usize) -> Hit,
        x: f32,
        mut y: f32,
        w: f32,
    ) -> f32 {
        let t = &m.theme;
        ui.text(tr(m.lang, title), x, y, SECTION, t.text_dim);
        y += LABEL_HEIGHT;

        if devices.is_empty() {
            let line_h = ui.measure("Ag", BODY).1;
            ui.text(
                tr(m.lang, Key::NoDevice),
                x + 10.0,
                y + (ROW_HEIGHT - line_h) / 2.0,
                BODY,
                t.text_dim,
            );
            return y + ROW_HEIGHT;
        }

        for (i, name) in devices.iter().take(MAX_LISTED_DEVICES).enumerate() {
            let row = Rect::new(x, y, w, ROW_HEIGHT);
            self.register(hit(i), row);
            let is_selected = selected == Some(i);
            let (hover, press) = self.amounts(hit(i));
            let alpha = t.hover.a * (hover + 0.9 * press);
            if alpha > 0.002 {
                ui.rect(row, 9.0, t.hover.with_alpha(alpha.min(1.0)));
            }
            let label = fit_text(ui, name, BODY, w - 20.0 - 28.0);
            let line_h = ui.measure("Ag", BODY).1;
            let color = if is_selected { t.text } else { t.text_dim };
            ui.text(
                &label,
                x + 10.0,
                y + (ROW_HEIGHT - line_h) / 2.0,
                BODY,
                color,
            );
            if is_selected {
                ui.icon(
                    ICON_CHECK,
                    x + w - 18.0,
                    y + ROW_HEIGHT / 2.0,
                    15.0,
                    t.accent,
                );
            }
            y += ROW_HEIGHT;
        }
        y
    }

    #[allow(clippy::too_many_arguments)]
    fn segmented_section(
        &mut self,
        ui: &mut Ui,
        m: &Model,
        title: Key,
        labels: &[&str],
        selected: usize,
        position: f32,
        hit: fn(usize) -> Hit,
        x: f32,
        mut y: f32,
        w: f32,
    ) -> f32 {
        let t = &m.theme;
        ui.text(tr(m.lang, title), x, y, SECTION, t.text_dim);
        y += LABEL_HEIGHT;

        let track = Rect::new(x, y, w, ROW_HEIGHT);
        ui.rect(track, 11.0, t.segment_track);
        let segment_w = (w - 6.0) / labels.len().max(1) as f32;
        let style = TextStyle::new(12.5, Weight::Medium);
        let line_h = ui.measure("Ag", style).1;

        // The active indicator glides between segments.
        let indicator = Rect::new(
            x + 3.0 + segment_w * position,
            y + 3.0,
            segment_w,
            ROW_HEIGHT - 6.0,
        );
        ui.rect(indicator, 8.0, t.segment_active);

        for (i, label) in labels.iter().enumerate() {
            let segment = Rect::new(
                x + 3.0 + segment_w * i as f32,
                y + 3.0,
                segment_w,
                ROW_HEIGHT - 6.0,
            );
            self.register(hit(i), segment);
            let (hover, press) = self.amounts(hit(i));
            let alpha = t.hover.a * (hover + 0.9 * press);
            if i != selected && alpha > 0.002 {
                ui.rect(segment, 8.0, t.hover.with_alpha(alpha.min(1.0)));
            }
            let color = if i == selected { t.text } else { t.text_dim };
            let text = fit_text(ui, label, style, segment_w - 8.0);
            let text_w = ui.measure(&text, style).0;
            ui.text(
                &text,
                segment.x + (segment_w - text_w) / 2.0,
                y + (ROW_HEIGHT - line_h) / 2.0,
                style,
                color,
            );
        }
        y + ROW_HEIGHT
    }

    #[cfg(test)]
    fn rect_of(&self, hit: Hit) -> Rect {
        self.hits
            .iter()
            .rev()
            .find(|(h, _)| *h == hit)
            .map(|(_, r)| *r)
            .unwrap_or_else(|| panic!("{hit:?} was not drawn"))
    }
}

/// Moves `visibility` towards shown/hidden at the fade speed.
fn step_visibility(visibility: f32, show: bool, dt: f32) -> f32 {
    let step = dt / FADE_SECS;
    if show {
        (visibility + step).min(1.0)
    } else {
        (visibility - step).max(0.0)
    }
}

/// Blends two colours; `amount` 0.0 gives `a`, 1.0 gives `b`.
fn lerp_color(a: Color, b: Color, amount: f32) -> Color {
    let k = amount.clamp(0.0, 1.0);
    let mix = |x: f32, y: f32| x + (y - x) * k;
    Color {
        r: mix(a.r, b.r),
        g: mix(a.g, b.g),
        b: mix(a.b, b.b),
        a: mix(a.a, b.a),
    }
}

/// Exponential smoothing of `current` towards `target`; snaps when close.
fn smooth(current: f32, target: f32, dt: f32, rate: f32) -> f32 {
    let next = current + (target - current) * (1.0 - (-rate * dt).exp());
    if (target - next).abs() < 0.002 {
        target
    } else {
        next
    }
}

/// Moves `current` towards `target` by at most `step`.
fn move_toward(current: f32, target: f32, step: f32) -> f32 {
    if current < target {
        (current + step).min(target)
    } else {
        (current - step).max(target)
    }
}

/// Fast start, gentle landing.
fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

/// Rounds a 0–1 volume to whole percent so values stay stable and exact.
fn round_percent(value: f32) -> f32 {
    (value * 100.0).round() / 100.0
}

fn divider_at(ui: &mut Ui, theme: &Theme, x: f32, cy: f32) -> f32 {
    ui.rect(Rect::new(x + GAP, cy - 10.0, 1.0, 20.0), 0.5, theme.track);
    x + GAP * 2.0 + 1.0
}

/// Height of the settings popover for `m`.
fn popover_height(m: &Model) -> f32 {
    let list_rows = |n: usize| n.clamp(1, MAX_LISTED_DEVICES) as f32 * ROW_HEIGHT;
    let device = |n: usize| LABEL_HEIGHT + list_rows(n);
    let segmented = LABEL_HEIGHT + ROW_HEIGHT;
    POPOVER_PAD * 2.0
        + device(m.video_devices.len())
        + device(m.audio_devices.len())
        + device(m.mic_devices.len())
        + segmented * 4.0
        + SECTION_GAP * 7.0
        + ROW_HEIGHT
}

/// Shortens `text` with an ellipsis so it fits in `max_w` logical pixels.
fn fit_text(ui: &mut Ui, text: &str, style: TextStyle, max_w: f32) -> String {
    if ui.measure(text, style).0 <= max_w {
        return text.to_owned();
    }
    let chars: Vec<char> = text.chars().collect();
    for keep in (1..chars.len()).rev() {
        let candidate: String = chars[..keep]
            .iter()
            .collect::<String>()
            .trim_end()
            .to_owned()
            + "…";
        if ui.measure(&candidate, style).0 <= max_w {
            return candidate;
        }
    }
    "…".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;

    fn model() -> Model {
        Model {
            theme: Theme::dark(),
            lang: Lang::En,
            status: StatusKind::Live,
            resolution: "1080p · 60 fps".into(),
            volume: 0.5,
            sync_ms: 0,
            fullscreen: false,
            video_devices: vec!["UGREEN 25173".into(), "Webcam".into()],
            video_selected: Some(0),
            audio_devices: vec!["HDMI (UGREEN 25173)".into()],
            audio_selected: Some(0),
            mic_devices: vec!["Windows default".into(), "USB Microphone".into()],
            mic_selected: Some(0),
            preset: RenderPreset::Balanced,
            theme_mode: ThemeMode::Auto,
            lang_mode: LangMode::Auto,
            video_mode: VideoModePref::Auto,
            stats_open: false,
            replay: ReplayView::off("Ctrl+Alt+S"),
            mic: MicView::off(),
            rec: RecView::idle(),
        }
    }

    struct Rig {
        ui: Ui,
        controls: Controls,
        model: Model,
        pointer: Pointer,
    }

    impl Rig {
        fn new(always_visible: bool) -> Self {
            let mut rig = Self {
                ui: Ui::new(),
                controls: Controls::new(always_visible),
                model: model(),
                pointer: Pointer::default(),
            };
            rig.frame(0.016);
            rig
        }

        fn frame(&mut self, dt: f32) {
            self.ui.begin((1280, 720), 1.0);
            self.controls
                .frame(&mut self.ui, &self.model, &self.pointer, dt);
            self.pointer.end_frame();
        }

        fn move_to(&mut self, x: f32, y: f32) {
            self.pointer.pos = Some((x, y));
            self.frame(0.016);
        }

        fn click(&mut self, x: f32, y: f32) {
            self.move_to(x, y);
            self.pointer.press();
            self.frame(0.016);
            self.pointer.release();
            self.frame(0.016);
        }

        fn click_on(&mut self, hit: Hit) {
            let (x, y) = self.controls.rect_of(hit).center();
            self.click(x, y);
        }

        fn actions(&mut self) -> Vec<Action> {
            self.controls.take_actions()
        }
    }

    #[test]
    fn controls_fade_out_after_idle_and_draw_nothing() {
        let mut rig = Rig::new(false);
        rig.move_to(10.0, 10.0); // wake up
        for _ in 0..20 {
            rig.frame(0.1);
        }
        assert!(rig.controls.is_visible(), "still inside the idle window");
        for _ in 0..40 {
            rig.frame(0.1);
        }
        assert!(!rig.controls.is_visible());
        assert!(rig.ui.vertices().is_empty());
    }

    #[test]
    fn moving_the_pointer_wakes_hidden_controls() {
        let mut rig = Rig::new(false);
        for _ in 0..60 {
            rig.frame(0.1);
        }
        assert!(!rig.controls.is_visible());
        rig.move_to(300.0, 300.0);
        rig.move_to(310.0, 300.0);
        for _ in 0..20 {
            rig.frame(0.016);
        }
        assert!(rig.controls.is_visible());
    }

    #[test]
    fn preview_mode_never_hides() {
        let mut rig = Rig::new(true);
        for _ in 0..100 {
            rig.frame(0.1);
        }
        assert!(rig.controls.is_visible());
    }

    #[test]
    fn fade_steps_take_the_configured_time() {
        let mut v = 0.0;
        for _ in 0..10 {
            v = step_visibility(v, true, FADE_SECS / 10.0);
        }
        assert!((v - 1.0).abs() < 1e-4);
        assert_eq!(step_visibility(0.1, false, 1.0), 0.0);
    }

    #[test]
    fn clicking_settings_opens_then_closes_the_popover() {
        let mut rig = Rig::new(true);
        assert!(!rig.controls.settings_open());
        rig.click_on(Hit::Settings);
        assert!(rig.controls.settings_open());
        rig.click_on(Hit::Settings);
        assert!(!rig.controls.settings_open());
    }

    #[test]
    fn clicking_outside_closes_the_popover() {
        let mut rig = Rig::new(true);
        rig.click_on(Hit::Settings);
        assert!(rig.controls.settings_open());
        rig.click(40.0, 40.0);
        assert!(!rig.controls.settings_open());
    }

    #[test]
    fn clicking_inside_the_popover_keeps_it_open() {
        let mut rig = Rig::new(true);
        rig.click_on(Hit::Settings);
        let popover = rig.controls.rect_of(Hit::Popover);
        rig.click(popover.x + 4.0, popover.y + 4.0);
        assert!(rig.controls.settings_open());
    }

    #[test]
    fn dragging_the_slider_sets_the_volume_from_the_pointer() {
        let mut rig = Rig::new(true);
        let track = rig.controls.volume_track;
        rig.move_to(track.x + track.w * 0.25, track.y);
        rig.pointer.press();
        rig.frame(0.016);
        rig.move_to(track.x + track.w * 0.75, track.y);
        rig.pointer.release();
        rig.frame(0.016);

        let volumes: Vec<f32> = rig
            .actions()
            .into_iter()
            .filter_map(|a| {
                if let Action::SetVolume(v) = a {
                    Some(v)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(volumes.first().copied(), Some(0.25));
        assert_eq!(volumes.last().copied(), Some(0.75));
    }

    #[test]
    fn dragging_past_the_ends_clamps_the_volume() {
        let mut rig = Rig::new(true);
        let track = rig.controls.volume_track;
        rig.move_to(track.x + 10.0, track.y);
        rig.pointer.press();
        rig.frame(0.016);
        rig.move_to(track.x + track.w + 200.0, track.y);
        let last = rig.actions().into_iter().rev().find_map(|a| match a {
            Action::SetVolume(v) => Some(v),
            _ => None,
        });
        assert_eq!(last, Some(1.0));
    }

    #[test]
    fn scrolling_over_the_pill_changes_the_volume() {
        let mut rig = Rig::new(true);
        let pill = rig.controls.rect_of(Hit::Pill);
        rig.move_to(pill.x + 30.0, pill.y + 28.0);
        rig.pointer.wheel = 1.0;
        rig.frame(0.016);
        assert_eq!(rig.actions(), vec![Action::SetVolume(0.55)]);
    }

    #[test]
    fn scrolling_away_from_the_controls_does_nothing() {
        let mut rig = Rig::new(true);
        rig.move_to(20.0, 20.0);
        rig.pointer.wheel = 1.0;
        rig.frame(0.016);
        assert!(rig.actions().is_empty());
    }

    #[test]
    fn sync_buttons_step_by_ten_and_clamp() {
        let mut rig = Rig::new(true);
        rig.click_on(Hit::SyncPlus);
        assert_eq!(rig.actions(), vec![Action::SetSync(10)]);
        rig.click_on(Hit::SyncMinus);
        assert_eq!(rig.actions(), vec![Action::SetSync(-10)]);

        rig.model.sync_ms = MAX_AUDIO_OFFSET_MS;
        rig.frame(0.016);
        rig.click_on(Hit::SyncPlus);
        assert_eq!(rig.actions(), vec![Action::SetSync(MAX_AUDIO_OFFSET_MS)]);
    }

    #[test]
    fn the_video_mode_selector_emits_the_choice() {
        let mut rig = Rig::new(true);
        rig.click_on(Hit::Settings);
        rig.click_on(Hit::VideoMode(1));
        assert_eq!(
            rig.actions(),
            vec![Action::SetVideoMode(VideoModePref::Compressed)]
        );
        rig.click_on(Hit::VideoMode(0));
        assert_eq!(
            rig.actions(),
            vec![Action::SetVideoMode(VideoModePref::Auto)]
        );
    }

    #[test]
    fn a_two_option_selector_slides_to_its_second_segment() {
        let mut rig = Rig::new(true);
        rig.click_on(Hit::Settings);
        for _ in 0..30 {
            rig.frame(0.016);
        }
        assert_eq!(rig.controls.segments.unwrap()[3], 0.0);
        rig.model.video_mode = VideoModePref::Compressed;
        for _ in 0..60 {
            rig.frame(0.016);
        }
        assert_eq!(rig.controls.segments.unwrap()[3], 1.0);
    }

    fn recording_model(rig: &mut Rig) {
        rig.model.replay = ReplayView {
            state: ReplayState::Recording,
            buffered_seconds: 42.0,
            buffered_mb: 210.0,
            keep_seconds: 30,
            folder: r"C:\Users\Someone\Videos\Dockview".into(),
            ..ReplayView::off("Ctrl+Alt+S")
        };
        rig.frame(0.016);
    }

    #[test]
    fn the_replay_button_opens_its_own_popover_and_closes_settings() {
        let mut rig = Rig::new(true);
        rig.click_on(Hit::Settings);
        assert!(rig.controls.settings_open() && !rig.controls.replay_open());
        rig.click_on(Hit::Replay);
        assert!(rig.controls.replay_open(), "replay popover is open");
        assert!(!rig.controls.settings_open(), "only one popover at a time");
        rig.click_on(Hit::Replay);
        assert!(!rig.controls.replay_open(), "the button toggles it closed");
    }

    #[test]
    fn clicking_outside_closes_the_replay_popover_too() {
        let mut rig = Rig::new(true);
        rig.click_on(Hit::Replay);
        assert!(rig.controls.replay_open());
        rig.click(40.0, 40.0);
        assert!(!rig.controls.replay_open());
    }

    #[test]
    fn escape_style_closing_handles_either_popover() {
        let mut rig = Rig::new(true);
        assert!(!rig.controls.close_settings(), "nothing was open");
        rig.click_on(Hit::Replay);
        assert!(rig.controls.close_settings());
        rig.click_on(Hit::Settings);
        assert!(rig.controls.close_settings());
    }

    #[test]
    fn the_record_button_emits_its_action_in_every_state() {
        for state in [RecState::Idle, RecState::Starting, RecState::Recording] {
            let mut rig = Rig::new(true);
            rig.model.rec = RecView {
                state,
                seconds: 42.0,
            };
            rig.frame(0.016);
            rig.click_on(Hit::Record);
            assert_eq!(rig.actions(), vec![Action::ToggleRecord], "{state:?}");
            assert_eq!(rig.model.rec.is_on(), state != RecState::Idle);
        }
    }

    #[test]
    fn the_microphone_switch_and_device_list_emit_their_actions() {
        let mut rig = Rig::new(true);
        recording_model(&mut rig);
        rig.click_on(Hit::Replay);
        rig.click_on(Hit::MicToggle);
        assert_eq!(rig.actions(), vec![Action::ToggleMic]);
        rig.click_on(Hit::Settings);
        rig.click_on(Hit::MicDevice(1));
        assert_eq!(rig.actions(), vec![Action::SelectMic(1)]);
    }

    #[test]
    fn the_microphone_states_draw_without_breaking_the_popover() {
        for state in [
            MicState::Off,
            MicState::Starting,
            MicState::Live,
            MicState::Unavailable,
        ] {
            let mut rig = Rig::new(true);
            recording_model(&mut rig);
            rig.model.mic = MicView { state, level: 0.6 };
            rig.click_on(Hit::Replay);
            rig.frame(0.016);
            assert!(rig.controls.replay_open(), "{state:?}");
            assert_eq!(rig.model.mic.is_on(), state != MicState::Off);
        }
    }

    #[test]
    fn the_replay_popover_controls_emit_their_actions() {
        let mut rig = Rig::new(true);
        recording_model(&mut rig);
        rig.click_on(Hit::Replay);
        rig.click_on(Hit::ReplayToggle);
        rig.click_on(Hit::ReplayShorter);
        rig.click_on(Hit::ReplayLonger);
        rig.click_on(Hit::ReplayPopSave);
        rig.click_on(Hit::ReplayFolder);
        assert_eq!(
            rig.actions(),
            vec![
                Action::ToggleReplay,
                Action::ReplayLength(-1),
                Action::ReplayLength(1),
                Action::SaveReplay,
                Action::OpenReplayFolder,
            ]
        );
        assert!(
            rig.controls.replay_open(),
            "using the options keeps it open"
        );
    }

    #[test]
    fn the_pill_save_button_emits_a_save_even_when_the_buffer_is_off() {
        // The app answers with "turn on instant replay first".
        let mut rig = Rig::new(true);
        rig.click_on(Hit::ReplaySave);
        assert_eq!(rig.actions(), vec![Action::SaveReplay]);
    }

    #[test]
    fn the_replay_popover_stays_inside_the_window_in_both_languages_and_themes() {
        for lang in [Lang::En, Lang::Es] {
            for theme in [Theme::dark(), Theme::light()] {
                let mut rig = Rig::new(true);
                recording_model(&mut rig);
                rig.model.lang = lang;
                rig.model.theme = theme;
                rig.frame(0.016);
                rig.click_on(Hit::Replay);
                let popover = rig.controls.rect_of(Hit::Popover);
                assert!(
                    popover.x >= 0.0 && popover.x + popover.w <= 1280.0,
                    "{lang:?}"
                );
                assert!(
                    popover.y >= 0.0 && popover.y + popover.h <= 720.0,
                    "{lang:?}"
                );
            }
        }
    }

    #[test]
    fn the_pill_still_fits_with_the_replay_buttons_in_both_languages() {
        for lang in [Lang::En, Lang::Es] {
            let mut rig = Rig::new(true);
            rig.model.lang = lang;
            rig.frame(0.016);
            let pill = rig.controls.rect_of(Hit::Pill);
            assert!(
                pill.x >= 0.0 && pill.x + pill.w <= 1280.0,
                "{lang:?}: {pill:?}"
            );
        }
    }

    #[test]
    fn the_switch_knob_glides_between_off_and_on() {
        let mut rig = Rig::new(true);
        rig.click_on(Hit::Replay);
        for _ in 0..20 {
            rig.frame(0.016);
        }
        let off = rig.controls.amounts[&Hit::ReplayKnob].0;
        assert_eq!(off, 0.0);
        recording_model(&mut rig);
        let mid = rig.controls.amounts[&Hit::ReplayKnob].0;
        assert!(mid > 0.0 && mid < 1.0, "gliding, got {mid}");
        for _ in 0..60 {
            rig.frame(0.016);
        }
        assert_eq!(rig.controls.amounts[&Hit::ReplayKnob].0, 1.0);
    }

    #[test]
    fn colours_blend_linearly_and_clamp() {
        let (a, b) = (Color::hex(0x000000, 0.0), Color::hex(0xFFFFFF, 1.0));
        let mid = lerp_color(a, b, 0.5);
        assert!((mid.r - 0.5).abs() < 1e-6 && (mid.a - 0.5).abs() < 1e-6);
        assert_eq!(lerp_color(a, b, -3.0), a);
        assert_eq!(lerp_color(a, b, 9.0), b);
    }

    #[test]
    fn the_statistics_row_toggles_the_panel() {
        let mut rig = Rig::new(true);
        rig.click_on(Hit::Settings);
        rig.click_on(Hit::Stats);
        assert_eq!(rig.actions(), vec![Action::ToggleStats]);
        assert!(
            rig.controls.settings_open(),
            "toggling stats keeps the popover"
        );
    }

    #[test]
    fn fullscreen_button_emits_an_action() {
        let mut rig = Rig::new(true);
        rig.click_on(Hit::Fullscreen);
        assert_eq!(rig.actions(), vec![Action::ToggleFullscreen]);
    }

    #[test]
    fn releasing_elsewhere_cancels_a_click() {
        let mut rig = Rig::new(true);
        let (x, y) = rig.controls.rect_of(Hit::Fullscreen).center();
        rig.move_to(x, y);
        rig.pointer.press();
        rig.frame(0.016);
        rig.move_to(30.0, 30.0);
        rig.pointer.release();
        rig.frame(0.016);
        assert!(rig.actions().is_empty());
    }

    #[test]
    fn popover_selections_emit_the_matching_actions() {
        let mut rig = Rig::new(true);
        rig.click_on(Hit::Settings);

        rig.click_on(Hit::Video(1));
        rig.click_on(Hit::Audio(0));
        rig.click_on(Hit::Preset(2));
        rig.click_on(Hit::Theme(2));
        rig.click_on(Hit::Lang(2));
        assert_eq!(
            rig.actions(),
            vec![
                Action::SelectVideo(1),
                Action::SelectAudio(0),
                Action::SetPreset(RenderPreset::Quality),
                Action::SetTheme(ThemeMode::Light),
                Action::SetLang(LangMode::Es),
            ]
        );
        assert!(rig.controls.settings_open(), "selecting must not close it");
    }

    #[test]
    fn popover_stays_inside_the_window_in_both_languages() {
        for lang in [Lang::En, Lang::Es] {
            let mut rig = Rig::new(true);
            rig.model.lang = lang;
            rig.frame(0.016); // lay out with the new language before clicking
            rig.click_on(Hit::Settings);
            let popover = rig.controls.rect_of(Hit::Popover);
            assert!(
                popover.x >= 0.0 && popover.x + popover.w <= 1280.0,
                "{lang:?}"
            );
            assert!(
                popover.y >= 0.0 && popover.y + popover.h <= 720.0,
                "{lang:?}"
            );
        }
    }

    #[test]
    fn long_device_names_are_truncated_with_an_ellipsis() {
        let mut ui = Ui::new();
        ui.begin((1280, 720), 1.0);
        let long = "A very long capture device name that cannot possibly fit";
        let fitted = fit_text(&mut ui, long, BODY, 120.0);
        assert!(fitted.ends_with('…') && fitted.chars().count() < long.chars().count());
        assert!(ui.measure(&fitted, BODY).0 <= 120.0);
        assert_eq!(fit_text(&mut ui, "Short", BODY, 120.0), "Short");
    }

    #[test]
    fn an_empty_device_list_still_has_room_for_a_placeholder() {
        let mut m = model();
        let with_one = popover_height(&m);
        m.audio_devices.clear();
        assert_eq!(
            popover_height(&m),
            with_one,
            "empty shows one placeholder row"
        );
        m.audio_devices = (0..9).map(|i| format!("Device {i}")).collect();
        assert_eq!(
            popover_height(&m),
            with_one + ROW_HEIGHT * (MAX_LISTED_DEVICES as f32 - 1.0)
        );
    }

    #[test]
    fn pill_width_does_not_change_when_numbers_change() {
        let mut rig = Rig::new(true);
        let before = rig.controls.rect_of(Hit::Pill);
        rig.model.volume = 1.0;
        rig.model.sync_ms = -150;
        rig.model.resolution = "720p · 30 fps".into();
        rig.frame(0.016);
        assert_eq!(rig.controls.rect_of(Hit::Pill), before);
    }

    #[test]
    fn pill_fits_a_small_window_in_both_languages() {
        for lang in [Lang::En, Lang::Es] {
            let mut rig = Rig::new(true);
            rig.model.lang = lang;
            rig.frame(0.016);
            let pill = rig.controls.rect_of(Hit::Pill);
            assert!(pill.x >= 0.0 && pill.x + pill.w <= 1280.0, "{lang:?}");
        }
    }

    #[test]
    fn spanish_pill_is_wider_than_english() {
        let mut en = Rig::new(true);
        let mut es = Rig::new(true);
        es.model.lang = Lang::Es;
        es.frame(0.016);
        en.frame(0.016);
        assert!(es.controls.rect_of(Hit::Pill).w > en.controls.rect_of(Hit::Pill).w);
    }

    #[test]
    fn smoothing_converges_monotonically_and_snaps() {
        let mut v = 0.0;
        let mut last = 0.0;
        for _ in 0..120 {
            v = smooth(v, 1.0, 0.016, HOVER_RATE);
            assert!(v >= last && v <= 1.0);
            last = v;
        }
        assert_eq!(v, 1.0, "close values snap exactly onto the target");
        assert!(
            smooth(0.0, 1.0, 0.016, HOVER_RATE) < 0.5,
            "one frame is a small step"
        );
    }

    #[test]
    fn easing_endpoints_and_direction() {
        assert_eq!(ease_out_cubic(0.0), 0.0);
        assert_eq!(ease_out_cubic(1.0), 1.0);
        assert!(ease_out_cubic(0.5) > 0.5, "ease-out leads a linear ramp");
        assert_eq!(move_toward(0.9, 1.0, 0.5), 1.0);
        assert_eq!(move_toward(0.2, 0.0, 0.5), 0.0);
    }

    #[test]
    fn hover_fades_in_and_out_instead_of_jumping() {
        let mut rig = Rig::new(true);
        let (x, y) = rig.controls.rect_of(Hit::Fullscreen).center();
        rig.move_to(x, y);
        rig.frame(0.016);
        let early = rig.controls.amounts[&Hit::Fullscreen].0;
        assert!(early > 0.0 && early < 1.0, "mid-fade, got {early}");
        for _ in 0..30 {
            rig.frame(0.016);
        }
        assert_eq!(rig.controls.amounts[&Hit::Fullscreen].0, 1.0);

        rig.move_to(20.0, 20.0);
        rig.frame(0.016);
        let leaving = rig.controls.amounts[&Hit::Fullscreen].0;
        assert!(leaving > 0.0 && leaving < 1.0, "fading out, got {leaving}");
    }

    #[test]
    fn pressing_a_button_dips_it() {
        let mut rig = Rig::new(true);
        let (x, y) = rig.controls.rect_of(Hit::Fullscreen).center();
        rig.move_to(x, y);
        rig.pointer.press();
        for _ in 0..10 {
            rig.frame(0.016);
        }
        assert!(rig.controls.amounts[&Hit::Fullscreen].1 > 0.5);
    }

    #[test]
    fn popover_opens_over_the_configured_time_and_fades_out_non_interactively() {
        let mut rig = Rig::new(true);
        rig.click_on(Hit::Settings);
        assert!(rig.controls.popover_t > 0.0 && rig.controls.popover_t < 1.0);
        for _ in 0..20 {
            rig.frame(0.016);
        }
        assert_eq!(rig.controls.popover_t, 1.0);

        rig.click_on(Hit::Settings); // close
        assert!(!rig.controls.settings_open());
        assert!(rig.controls.popover_t > 0.0, "still fading out");
        let drawn = rig.ui.vertices().len();
        assert!(drawn > 0);
        assert!(
            rig.controls.hits.iter().all(|(h, _)| !h.in_popover()),
            "a closing popover must not take clicks"
        );
        for _ in 0..30 {
            rig.frame(0.016);
        }
        assert_eq!(rig.controls.popover_t, 0.0);
        assert!(
            rig.controls.segments.is_none() && rig.controls.amounts.keys().all(|h| !h.in_popover())
        );
    }

    #[test]
    fn selection_indicator_slides_between_segments() {
        let mut rig = Rig::new(true);
        rig.click_on(Hit::Settings);
        for _ in 0..30 {
            rig.frame(0.016);
        }
        assert_eq!(rig.controls.segments.unwrap()[1], 0.0, "starts on Auto");

        rig.model.theme_mode = ThemeMode::Light; // as if the click was applied
        rig.frame(0.016);
        let middle = rig.controls.segments.unwrap()[1];
        assert!(middle > 0.0 && middle < 2.0, "sliding, got {middle}");
        for _ in 0..60 {
            rig.frame(0.016);
        }
        assert_eq!(rig.controls.segments.unwrap()[1], 2.0);
    }

    #[test]
    fn indicator_starts_on_the_current_choice_without_sliding_in() {
        let mut rig = Rig::new(true);
        rig.model.lang_mode = LangMode::Es;
        rig.frame(0.016);
        rig.click_on(Hit::Settings);
        assert_eq!(rig.controls.segments.unwrap()[2], 2.0);
    }

    #[test]
    fn disabling_animations_makes_everything_instant() {
        let mut rig = Rig::new(true);
        rig.controls.set_animations(false);
        rig.click_on(Hit::Settings);
        assert_eq!(rig.controls.popover_t, 1.0);
        let (x, y) = rig.controls.rect_of(Hit::Fullscreen).center();
        rig.move_to(x, y);
        assert_eq!(rig.controls.amounts[&Hit::Fullscreen].0, 1.0);
        rig.model.theme_mode = ThemeMode::Dark;
        rig.frame(0.016);
        assert_eq!(rig.controls.segments.unwrap()[1], 1.0);
    }

    #[test]
    fn connecting_status_pulses_but_live_does_not() {
        let alpha_of_dot = |rig: &Rig| {
            rig.ui
                .vertices()
                .iter()
                .find(|v| v.shape[3] == 0.0 && v.shape[0] == 4.0)
                .map(|v| v.color[3])
        };
        let mut rig = Rig::new(true);
        rig.model.status = StatusKind::Connecting;
        let mut seen = Vec::new();
        for _ in 0..40 {
            rig.frame(0.05);
            seen.push(alpha_of_dot(&rig).unwrap());
        }
        let (lo, hi) = seen
            .iter()
            .fold((f32::MAX, f32::MIN), |(lo, hi), a| (lo.min(*a), hi.max(*a)));
        assert!(hi - lo > 0.3, "pulse range {lo}..{hi}");

        rig.model.status = StatusKind::Live;
        rig.frame(0.05);
        assert_eq!(alpha_of_dot(&rig), Some(1.0));
    }

    #[test]
    fn wants_pointer_only_over_widgets_not_backgrounds() {
        let mut rig = Rig::new(true);
        let pill = rig.controls.rect_of(Hit::Pill);
        rig.move_to(pill.x + 8.0, pill.y + 2.0); // pill background corner
        assert!(!rig.controls.wants_pointer());
        let (x, y) = rig.controls.rect_of(Hit::Fullscreen).center();
        rig.move_to(x, y);
        assert!(rig.controls.wants_pointer());
    }
}
