//! Helpers for the tests that need a GPU, a hardware encoder or a screen.
//!
//! Those tests already skip by themselves on a PC without the hardware. Setting
//! `DOCKVIEW_NO_HARDWARE=1` skips them on purpose: a CI runner has none, and a
//! quiet run (while the PC is busy with a game) must not load the GPU.

/// Name of the variable that skips the hardware tests.
pub(crate) const NO_HARDWARE: &str = "DOCKVIEW_NO_HARDWARE";

/// Whether a value of the variable means "skip": anything but empty or `0`.
fn means_skip(value: Option<&str>) -> bool {
    value.is_some_and(|v| !v.trim().is_empty() && v.trim() != "0")
}

/// `true` when the test may use the hardware it needs (`what`). When it must
/// not, says so and returns `false`; the test then returns early.
pub(crate) fn hardware_or_skip(what: &str) -> bool {
    let skip = means_skip(std::env::var(NO_HARDWARE).ok().as_deref());
    if skip {
        eprintln!("{NO_HARDWARE} is set: skipping, this test needs {what}");
    }
    !skip
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_real_value_means_skip() {
        assert!(!means_skip(None));
        assert!(!means_skip(Some("")));
        assert!(!means_skip(Some("  ")));
        assert!(!means_skip(Some("0")));
        assert!(means_skip(Some("1")));
        assert!(means_skip(Some("true")));
    }
}
