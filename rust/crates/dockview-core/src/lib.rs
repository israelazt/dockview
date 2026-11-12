//! Dockview core: capture, audio, stream selection and settings.
//! Platform-independent logic lives here so it can be unit tested.

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    #[test]
    fn has_version() {
        assert!(!super::version().is_empty());
    }
}
