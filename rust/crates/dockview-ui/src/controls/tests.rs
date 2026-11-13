//! Tests of the live controls.

use super::popover_settings::*;
use super::tooltip::*;
use super::*;
use dockview_domain::i18n::Lang;
use dockview_domain::settings::{LangMode, RecordingQuality, RenderPreset, VideoModePref};
use dockview_domain::theme::{Theme, ThemeMode};

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
        sources: SourcesView::default(),
        scenes: ScenesView::default(),
    }
}

pub(super) struct Rig {
    ui: Ui,
    pub(super) controls: Controls,
    pub(super) model: Model,
    pointer: Pointer,
}

impl Rig {
    pub(super) fn new(always_visible: bool) -> Self {
        let mut rig = Self {
            ui: Ui::new(),
            controls: Controls::new(always_visible),
            model: model(),
            pointer: Pointer::default(),
        };
        rig.frame(0.016);
        rig
    }

    pub(super) fn frame(&mut self, dt: f32) {
        self.ui.begin((1280, 720), 1.0);
        self.controls
            .frame(&mut self.ui, &self.model, &self.pointer, dt);
        self.pointer.end_frame();
    }

    pub(super) fn move_to(&mut self, x: f32, y: f32) {
        self.pointer.pos = Some((x, y));
        self.frame(0.016);
    }

    pub(super) fn click(&mut self, x: f32, y: f32) {
        self.move_to(x, y);
        self.pointer.press();
        self.frame(0.016);
        self.pointer.release();
        self.frame(0.016);
    }

    pub(super) fn click_on(&mut self, hit: Hit) {
        let (x, y) = self.controls.rect_of(hit).center();
        self.click(x, y);
    }

    pub(super) fn actions(&mut self) -> Vec<Action> {
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

fn many_sources() -> SourcesView {
    SourcesView {
        monitors: vec![
            "Screen 1 · 1920×1080 (primary)".into(),
            "Screen 2 · 1920×1080".into(),
        ],
        windows: (0..12)
            .map(|i| format!("Application window number {i}"))
            .collect(),
        current: CurrentSource::Capture,
    }
}

#[test]
fn every_control_has_a_tooltip_in_both_languages_with_nothing_left_to_fill_in() {
    let hits = [
        Hit::Sources,
        Hit::Replay,
        Hit::ReplaySave,
        Hit::ReplayPopSave,
        Hit::Record,
        Hit::Settings,
        Hit::Fullscreen,
        Hit::Volume,
        Hit::SyncMinus,
        Hit::SyncPlus,
        Hit::ReplayToggle,
        Hit::ReplayShorter,
        Hit::ReplayLonger,
        Hit::MicToggle,
        Hit::ReplayFolder,
        Hit::ReplayChangeFolder,
        Hit::Quality(0),
        Hit::Quality(1),
        Hit::VideoMode(0),
        Hit::VideoMode(1),
        Hit::Stats,
        Hit::Preset(0),
        Hit::Preset(1),
        Hit::Preset(2),
    ];
    for lang in [Lang::En, Lang::Es] {
        let mut m = model();
        m.lang = lang;
        m.replay = ReplayView {
            record_hotkey: "Ctrl+Alt+R".into(),
            ..ReplayView::off("Ctrl+Alt+S")
        };
        for hit in hits {
            let text = tooltip_for(hit, &m).unwrap_or_else(|| panic!("{hit:?} has no tooltip"));
            assert!(
                !text.trim().is_empty() && !text.contains(['{', '}']),
                "{lang:?} {hit:?}: {text}"
            );
        }
    }
    // Backgrounds and plain list rows say nothing.
    let m = model();
    assert_eq!(tooltip_for(Hit::Pill, &m), None);
    assert_eq!(tooltip_for(Hit::Popover, &m), None);
    assert_eq!(tooltip_for(Hit::Video(0), &m), None);
}

#[test]
fn tooltips_name_the_hotkey_and_follow_the_state() {
    let mut m = model();
    m.replay = ReplayView {
        state: ReplayState::Recording,
        keep_seconds: 30,
        record_hotkey: "Ctrl+Alt+R".into(),
        ..ReplayView::off("Ctrl+Alt+S")
    };
    let save = tooltip_for(Hit::ReplaySave, &m).unwrap();
    assert!(
        save.contains("30 s") && save.contains("Ctrl+Alt+S"),
        "{save}"
    );
    assert!(tooltip_for(Hit::Record, &m)
        .unwrap()
        .contains("Start recording (Ctrl+Alt+R)"));
    m.rec.state = RecState::Recording;
    assert!(tooltip_for(Hit::Record, &m)
        .unwrap()
        .contains("Stop recording"));
    m.fullscreen = true;
    assert!(tooltip_for(Hit::Fullscreen, &m)
        .unwrap()
        .contains("Leave fullscreen"));
    m.replay.state = ReplayState::Off;
    assert!(tooltip_for(Hit::ReplaySave, &m).unwrap().contains("off"));
}

#[test]
fn a_tooltip_appears_after_resting_on_a_button_and_not_before() {
    let mut rig = Rig::new(true);
    let (x, y) = rig.controls.rect_of(Hit::Fullscreen).center();
    rig.move_to(x, y);
    let before = rig.ui.vertices().len();
    rig.frame(0.2);
    assert_eq!(rig.ui.vertices().len(), before, "too early for a tooltip");
    rig.frame(0.4); // 0.6 s resting
    rig.frame(0.2);
    assert!(rig.ui.vertices().len() > before, "the tooltip is drawn");
}

#[test]
fn pressing_hides_the_tooltip_and_moving_away_resets_the_wait() {
    let mut rig = Rig::new(true);
    let (x, y) = rig.controls.rect_of(Hit::Settings).center();
    rig.move_to(x, y);
    rig.frame(0.6);
    rig.frame(0.1);
    let shown = rig.ui.vertices().len();
    rig.pointer.press();
    rig.frame(0.1);
    assert!(rig.ui.vertices().len() < shown, "no tooltip while pressed");
    rig.pointer.release();
    // Another button: the wait starts again.
    let (x2, y2) = rig.controls.rect_of(Hit::Fullscreen).center();
    rig.move_to(x2, y2);
    rig.frame(0.1);
    let early = rig.ui.vertices().len();
    rig.frame(0.5);
    rig.frame(0.1);
    assert!(rig.ui.vertices().len() > early);
}

#[test]
fn a_tooltip_stays_inside_the_window_even_on_the_last_button() {
    for lang in [Lang::En, Lang::Es] {
        let mut rig = Rig::new(true);
        rig.model.lang = lang;
        rig.frame(0.016);
        let button = rig.controls.rect_of(Hit::Fullscreen);
        let (x, y) = button.center();
        rig.move_to(x, y);
        rig.frame(0.7);
        rig.frame(0.2);
        let max_x = rig
            .ui
            .vertices()
            .iter()
            .map(|v| v.pos[0])
            .fold(0.0f32, f32::max);
        assert!(max_x <= 1280.0 + 2.0, "{lang:?}: drawn up to x={max_x}");
    }
}

#[test]
fn the_source_popover_emits_a_choice_for_each_kind_of_source() {
    let mut rig = Rig::new(true);
    rig.model.sources = many_sources();
    rig.click_on(Hit::Sources);
    assert!(rig.controls.sources_open());
    rig.click_on(Hit::Video(0));
    rig.click_on(Hit::SourceMonitor(1));
    rig.click_on(Hit::SourceWindow(3));
    assert_eq!(
        rig.actions(),
        vec![
            Action::SelectVideo(0),
            Action::SelectMonitor(1),
            Action::SelectWindow(3),
        ]
    );
    assert!(
        rig.controls.sources_open(),
        "choosing keeps the popover open"
    );
}

#[test]
fn only_one_popover_is_open_at_a_time_including_sources() {
    let mut rig = Rig::new(true);
    rig.click_on(Hit::Settings);
    rig.click_on(Hit::Sources);
    assert!(rig.controls.sources_open() && !rig.controls.settings_open());
    rig.click_on(Hit::Replay);
    assert!(rig.controls.replay_open() && !rig.controls.sources_open());
    rig.click_on(Hit::Sources);
    assert!(rig.controls.sources_open() && !rig.controls.replay_open());
    rig.click(40.0, 40.0);
    assert!(!rig.controls.sources_open(), "a click outside closes it");
}

#[test]
fn the_source_popover_with_many_windows_fits_a_720p_window() {
    for lang in [Lang::En, Lang::Es] {
        let mut rig = Rig::new(true);
        rig.model.lang = lang;
        rig.model.sources = many_sources();
        rig.model.sources.current = CurrentSource::Window(Some(2));
        rig.frame(0.016); // lay out for the new language before aiming at a button
        rig.click_on(Hit::Sources);
        rig.frame(0.016);
        let popover = rig.controls.rect_of(Hit::Popover);
        assert!(popover.y >= 0.0, "{lang:?}: top at {}", popover.y);
        assert!(popover.y + popover.h <= 720.0, "{lang:?}");
    }
}

#[test]
fn the_quality_and_folder_controls_emit_their_actions() {
    let mut rig = Rig::new(true);
    recording_model(&mut rig);
    rig.click_on(Hit::Replay);
    rig.click_on(Hit::Quality(0));
    rig.click_on(Hit::Quality(1));
    rig.click_on(Hit::ReplayChangeFolder);
    rig.click_on(Hit::ReplayFolder);
    assert_eq!(
        rig.actions(),
        vec![
            Action::SetQuality(RecordingQuality::Normal),
            Action::SetQuality(RecordingQuality::High),
            Action::ChooseFolder,
            Action::OpenReplayFolder,
        ]
    );
}

#[test]
fn the_replay_popover_with_quality_and_folder_fits_a_720p_window() {
    for lang in [Lang::En, Lang::Es] {
        let mut rig = Rig::new(true);
        rig.model.lang = lang;
        recording_model(&mut rig);
        rig.click_on(Hit::Replay);
        rig.frame(0.016);
        let popover = rig.controls.rect_of(Hit::Popover);
        assert!(popover.y >= 0.0, "{lang:?}: top at {}", popover.y);
        assert!(popover.y + popover.h <= 720.0, "{lang:?}");
    }
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
