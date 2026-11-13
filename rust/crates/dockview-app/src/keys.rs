//! What a key can ask for. Which key asks for what is the window's business
//! (the binary); the application only carries out the command.

/// Something a key asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCommand {
    ToggleFullscreen,
    /// Closes the settings first; leaves full screen if there was nothing to close.
    Escape,
    VolumeUp,
    VolumeDown,
    SyncEarlier,
    SyncLater,
    CyclePreset,
    ToggleStats,
    NextVideo,
    NextAudio,
}
