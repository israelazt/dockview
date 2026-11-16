//! Names to show for capture devices.
//!
//! The name Windows gives a cheap capture card comes from its firmware
//! ("C7 USB3.0 Video") and says nothing about what it is. Cards are recognised
//! by their USB vendor and product ids, which are the same on every PC and
//! every port, and shown with a better name: one of the user's own
//! (`deviceAliases` in the settings) or one from the table of known cards.
//! Only what is *shown* changes; the device is still found by its real name.

use std::collections::BTreeMap;

/// Capture cards that are named here, by `vid:pid` in lower case. Anyone can
/// add the card they own.
const KNOWN: &[(&str, &str)] = &[("345f:2131", "Pacoxi Budget HDMI Capture (USB3)")];

/// Longest alias kept, in characters.
pub const MAX_ALIAS_LEN: usize = 60;

/// The `vid:pid` of a USB device from the link Windows gives it, such as
/// `\\?\usb#vid_345f&pid_2131&mi_00#7&1644...`; `None` for anything else.
pub fn vid_pid(symbolic_link: &str) -> Option<String> {
    let link = symbolic_link.to_lowercase();
    let after = |text: &str, key: &str| -> Option<String> {
        let start = text.find(key)? + key.len();
        let id = text.get(start..start + 4)?;
        id.chars()
            .all(|c| c.is_ascii_hexdigit())
            .then(|| id.to_owned())
    };
    let vid_at = link.find("vid_")?;
    let vid = after(&link, "vid_")?;
    let pid = after(&link[vid_at..], "pid_")?;
    Some(format!("{vid}:{pid}"))
}

/// The name to show for a device: the user's alias if they set one, else the
/// known name for that card, else the name Windows gave it.
pub fn display_name(
    symbolic_link: &str,
    friendly_name: &str,
    aliases: &BTreeMap<String, String>,
) -> String {
    let Some(id) = vid_pid(symbolic_link) else {
        return friendly_name.to_owned();
    };
    if let Some(alias) = aliases.get(&id).map(|a| a.trim()).filter(|a| !a.is_empty()) {
        return alias.to_owned();
    }
    KNOWN
        .iter()
        .find(|(known, _)| *known == id)
        .map_or_else(|| friendly_name.to_owned(), |(_, name)| (*name).to_owned())
}

/// Cleans a set of aliases read from the settings file: lower-case ids,
/// trimmed and shortened names, nothing blank.
pub fn sanitize_aliases(aliases: BTreeMap<String, String>) -> BTreeMap<String, String> {
    aliases
        .into_iter()
        .filter_map(|(id, name)| {
            let id = id.trim().to_lowercase();
            let name: String = name.trim().chars().take(MAX_ALIAS_LEN).collect();
            (!id.is_empty() && !name.is_empty()).then_some((id, name))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PACOXI: &str = r"\\?\usb#vid_345f&pid_2131&mi_00#7&16449945&0&0000#{e5323777-f976-4f5b-9b55-b94699c46e44}\global";
    const WEBCAM: &str = r"\\?\usb#vid_1b3f&pid_2247&mi_00#8&2b31d917&0&0000#{e5323777-f976-4f5b-9b55-b94699c46e44}\global";

    #[test]
    fn the_ids_are_read_from_a_windows_link() {
        assert_eq!(vid_pid(PACOXI).as_deref(), Some("345f:2131"));
        assert_eq!(vid_pid(WEBCAM).as_deref(), Some("1b3f:2247"));
        assert_eq!(
            vid_pid(&PACOXI.to_uppercase()).as_deref(),
            Some("345f:2131")
        );
    }

    #[test]
    fn links_without_ids_are_not_recognised() {
        assert_eq!(vid_pid(""), None);
        assert_eq!(vid_pid(r"\\?\pci#something"), None);
        assert_eq!(vid_pid("usb#vid_12"), None);
        assert_eq!(vid_pid("usb#vid_zzzz&pid_0001"), None);
    }

    #[test]
    fn the_same_card_in_another_port_has_the_same_id() {
        let other_port = PACOXI.replace("7&16449945&0&0000", "3&aaaa1111&0&0004");
        assert_eq!(vid_pid(&other_port), vid_pid(PACOXI));
    }

    #[test]
    fn a_known_card_is_shown_with_its_own_name() {
        let none = BTreeMap::new();
        assert_eq!(
            display_name(PACOXI, "C7 USB3.0 Video", &none),
            "Pacoxi Budget HDMI Capture (USB3)"
        );
        // An unknown device keeps the name Windows gave it.
        assert_eq!(
            display_name(WEBCAM, "GENERAL WEBCAM", &none),
            "GENERAL WEBCAM"
        );
        assert_eq!(display_name("", "Whatever", &none), "Whatever");
    }

    #[test]
    fn the_users_alias_wins_over_everything() {
        let mut aliases = BTreeMap::new();
        aliases.insert(
            "345f:2131".to_owned(),
            "  Mi capturadora barata ".to_owned(),
        );
        aliases.insert("1b3f:2247".to_owned(), "Webcam del escritorio".to_owned());
        assert_eq!(
            display_name(PACOXI, "C7 USB3.0 Video", &aliases),
            "Mi capturadora barata"
        );
        assert_eq!(
            display_name(WEBCAM, "GENERAL WEBCAM", &aliases),
            "Webcam del escritorio"
        );
        // A blank alias falls back to the known name.
        aliases.insert("345f:2131".to_owned(), "   ".to_owned());
        assert_eq!(
            display_name(PACOXI, "C7 USB3.0 Video", &aliases),
            "Pacoxi Budget HDMI Capture (USB3)"
        );
    }

    #[test]
    fn aliases_from_a_file_are_cleaned_up() {
        let mut raw = BTreeMap::new();
        raw.insert(" 345F:2131 ".to_owned(), "  Nombre  ".to_owned());
        raw.insert("".to_owned(), "sin id".to_owned());
        raw.insert("1b3f:2247".to_owned(), "   ".to_owned());
        raw.insert("0000:0001".to_owned(), "x".repeat(200));
        let clean = sanitize_aliases(raw);
        assert_eq!(clean.len(), 2);
        assert_eq!(clean["345f:2131"], "Nombre");
        assert_eq!(clean["0000:0001"].chars().count(), MAX_ALIAS_LEN);
    }
}
