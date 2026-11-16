//! What the application is called on screen. One place, so the name is changed
//! in one line (the technical rename of the crates and the executable is a
//! separate step; see `REDESIGN_PLAN.md`, part R).

/// The product's name as the title bar and the taskbar show it.
pub const APP_NAME: &str = "Luminifex";

/// The version of this build, from the manifest.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The small label next to the name: `v0.1.0 · D3D11`.
pub fn version_label(renderer: &str) -> String {
    let renderer = renderer.trim();
    if renderer.is_empty() {
        format!("v{VERSION}")
    } else {
        format!("v{VERSION} · {renderer}")
    }
}

/// The text of the title bar, in capitals with the spacing the design asks for
/// drawn by the interface; this is only the letters.
pub fn title_text() -> String {
    APP_NAME.to_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_label_has_the_real_version_and_the_renderer() {
        let label = version_label("D3D11");
        assert!(label.starts_with('v'));
        assert!(label.ends_with(" · D3D11"));
        assert!(label.contains(VERSION));
    }

    #[test]
    fn no_renderer_leaves_only_the_version() {
        assert_eq!(version_label("  "), format!("v{VERSION}"));
    }

    #[test]
    fn the_title_is_the_name_in_capitals() {
        assert_eq!(title_text(), "LUMINIFEX");
    }
}
