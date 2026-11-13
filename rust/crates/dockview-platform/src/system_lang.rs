//! The language Windows is set to.

use dockview_domain::i18n::Lang;
use windows::Win32::Globalization::GetUserDefaultUILanguage;

/// The language of the Windows display (UI) language.
pub fn detect_system_lang() -> Lang {
    // SAFETY: simple query with no arguments.
    Lang::from_langid(unsafe { GetUserDefaultUILanguage() })
}
