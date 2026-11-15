//! What a key can ask for. Which key asks for what is the window's business
//! (the binary); the application only carries out the command.

/// Something a key asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCommand {
    ToggleFullscreen,
    /// Closes the settings first; then leaves full screen, or else closes the
    /// Studio panel, if there was nothing else to close.
    Escape,
    /// Opens or closes the Studio panel.
    ToggleStudio,
    VolumeUp,
    VolumeDown,
    SyncEarlier,
    SyncLater,
    /// Undoes the last move or resize of a layer in the editor.
    Undo,
    CyclePreset,
    ToggleStats,
    /// Shows or hides the row of readouts over the picture.
    ToggleHud,
    NextVideo,
    NextAudio,
}
