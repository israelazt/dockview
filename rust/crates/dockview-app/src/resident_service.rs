//! Dockview living on after its window is closed: the capture hotkey and the
//! tray icon. It keeps both in line with the settings (a new hotkey replaces the
//! old one; the tray appears or goes away) and turns what they report into what
//! the window should do. It only talks to the ports, so it is the same on every
//! platform and is tested with fakes.

use dockview_domain::capture_settings::CaptureSettings;
use dockview_domain::events::AppEvent;
use dockview_domain::hotkey::HotkeySpec;
use dockview_domain::i18n::Lang;
use dockview_domain::ports::{CaptureHotkeys, CaptureTrigger};
use dockview_domain::select::Mode;
use dockview_domain::tray::{Tray, TrayEvent, TrayHost, TrayLabels};

/// What the window is asked to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResidentEvent {
    /// Start a screenshot in this mode.
    Capture(Mode),
    /// Show the window.
    OpenWindow,
    /// End the program.
    Quit,
    /// The user clicked the notification of the last screenshot.
    NotificationClicked,
}

pub struct ResidentService {
    hotkeys: Box<dyn CaptureHotkeys>,
    tray_host: Box<dyn TrayHost>,
    trigger: Option<Box<dyn CaptureTrigger>>,
    /// The hotkey setting the trigger was made for (`None`: not looked at yet),
    /// so a hotkey that could not be had is not asked for again every tick.
    asked: Option<Option<HotkeySpec>>,
    tray: Option<Box<dyn Tray>>,
    tray_lang: Option<Lang>,
    /// The tray could not be shown with this setting; try again when it changes.
    tray_failed: bool,
}

impl ResidentService {
    pub fn new(hotkeys: Box<dyn CaptureHotkeys>, tray_host: Box<dyn TrayHost>) -> Self {
        Self {
            hotkeys,
            tray_host,
            trigger: None,
            asked: None,
            tray: None,
            tray_lang: None,
            tray_failed: false,
        }
    }

    /// Whether the icon is in the tray, so closing the window can leave
    /// Dockview running.
    pub fn keeps_in_tray(&self) -> bool {
        self.tray.is_some()
    }

    /// Brings the hotkey and the tray in line with `settings`; says what the
    /// user should know (a hotkey somebody else has).
    pub fn sync(&mut self, settings: &CaptureSettings, lang: Lang) -> Vec<AppEvent> {
        let mut events = Vec::new();
        if self.asked != Some(settings.hotkey) {
            self.asked = Some(settings.hotkey);
            // The old combination is given back before the new one is taken.
            self.trigger = None;
            if let Some(spec) = settings.hotkey {
                match self.hotkeys.register(&spec) {
                    Ok(trigger) => {
                        self.trigger = Some(trigger);
                        if self.hotkeys.kept_by_system(&spec) {
                            events.push(AppEvent::PrintScreenIsWindows);
                        }
                    }
                    Err(why) => {
                        dockview_domain::log_warn!(
                            "the capture hotkey {spec} was not registered: {why}"
                        );
                        events.push(AppEvent::CaptureHotkeyBusy);
                    }
                }
            }
        }
        if !settings.keep_in_tray {
            self.tray = None;
            self.tray_lang = None;
            self.tray_failed = false;
        } else if (self.tray.is_none() && !self.tray_failed) || self.tray_lang != Some(lang) {
            self.tray = None;
            match self.tray_host.show(&TrayLabels::new(lang)) {
                Ok(tray) => {
                    self.tray = Some(tray);
                    self.tray_failed = false;
                }
                Err(why) => {
                    dockview_domain::log_warn!("the tray icon was not shown: {why}");
                    self.tray_failed = true;
                }
            }
            self.tray_lang = Some(lang);
        }
        events
    }

    /// Shows a notification from the tray icon (when there is one).
    pub fn notify(&self, title: &str, text: &str) {
        if let Some(tray) = &self.tray {
            tray.notify(title, text);
        }
    }

    /// What the user did since the last call. The hotkey starts a capture in
    /// `mode`; the tray menu chooses its own.
    pub fn poll(&mut self, mode: Mode) -> Vec<ResidentEvent> {
        let mut events = Vec::new();
        if self.trigger.as_ref().is_some_and(|t| t.take_pressed()) {
            events.push(ResidentEvent::Capture(mode));
        }
        if let Some(tray) = &self.tray {
            events.extend(tray.poll().into_iter().map(|e| match e {
                TrayEvent::Open => ResidentEvent::OpenWindow,
                TrayEvent::Capture(m) => ResidentEvent::Capture(m),
                TrayEvent::Quit => ResidentEvent::Quit,
                TrayEvent::NotificationClicked => ResidentEvent::NotificationClicked,
            }));
        }
        events
    }
}
