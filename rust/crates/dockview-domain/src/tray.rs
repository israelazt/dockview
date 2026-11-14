//! The tray icon, as far as the application cares: what its menu offers and
//! what it reports. Showing it is the platform's job (a port).

use crate::i18n::{tr, Key, Lang};
use crate::select::Mode;

/// What the user did with the icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayEvent {
    /// Open (or bring forward) the Dockview window: a click on the icon.
    Open,
    /// Start a screenshot in this mode.
    Capture(Mode),
    /// Quit the program.
    Quit,
}

/// One entry of the menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuEntry {
    /// A line of text that cannot be chosen (a heading).
    Heading(String),
    /// Something to choose, and what choosing it reports.
    Item(String, TrayEvent),
    Separator,
}

/// The words of the icon and its menu, in one language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayLabels {
    pub tooltip: String,
    pub menu: Vec<MenuEntry>,
}

impl TrayLabels {
    pub fn new(lang: Lang) -> Self {
        let mut menu = vec![MenuEntry::Heading(tr(lang, Key::TrayNewCapture).to_owned())];
        menu.extend(Mode::ALL.iter().map(|mode| {
            MenuEntry::Item(tr(lang, mode.label()).to_owned(), TrayEvent::Capture(*mode))
        }));
        menu.push(MenuEntry::Separator);
        menu.push(MenuEntry::Item(
            tr(lang, Key::TrayOpen).to_owned(),
            TrayEvent::Open,
        ));
        menu.push(MenuEntry::Item(
            tr(lang, Key::TrayQuit).to_owned(),
            TrayEvent::Quit,
        ));
        Self {
            tooltip: tr(lang, Key::TrayTip).to_owned(),
            menu,
        }
    }
}

/// The icon in the tray.
pub trait Tray {
    /// What the user did since the last call, oldest first.
    fn poll(&self) -> Vec<TrayEvent>;
}

/// Shows the icon.
pub trait TrayHost {
    /// Puts the icon in the tray with `labels`; it goes away when the returned
    /// object is dropped.
    fn show(&self, labels: &TrayLabels) -> Result<Box<dyn Tray>, String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_menu_has_a_heading_one_item_per_mode_then_open_and_quit() {
        let labels = TrayLabels::new(Lang::En);
        assert_eq!(labels.menu.len(), 1 + 4 + 1 + 2);
        assert_eq!(labels.menu[0], MenuEntry::Heading("New screenshot".into()));
        let modes: Vec<TrayEvent> = labels
            .menu
            .iter()
            .filter_map(|e| match e {
                MenuEntry::Item(_, ev @ TrayEvent::Capture(_)) => Some(*ev),
                _ => None,
            })
            .collect();
        assert_eq!(modes, Mode::ALL.map(TrayEvent::Capture).to_vec());
        assert_eq!(labels.menu[5], MenuEntry::Separator);
        assert_eq!(
            labels.menu[6],
            MenuEntry::Item("Open Dockview".into(), TrayEvent::Open)
        );
        assert_eq!(
            labels.menu[7],
            MenuEntry::Item("Quit".into(), TrayEvent::Quit)
        );
    }

    #[test]
    fn it_speaks_the_language_asked() {
        let es = TrayLabels::new(Lang::Es);
        assert_eq!(es.menu[0], MenuEntry::Heading("Nueva captura".into()));
        assert!(es.menu.contains(&MenuEntry::Item(
            "Ventana".into(),
            TrayEvent::Capture(Mode::Window)
        )));
        assert!(es
            .menu
            .contains(&MenuEntry::Item("Salir".into(), TrayEvent::Quit)));
        assert_ne!(es.tooltip, "");
    }
}
