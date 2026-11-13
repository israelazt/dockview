//! The state of an audio input: starting, running, lost or failed.

/// Lifecycle of an an audio engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioStatus {
    Starting,
    Running,
    /// The capture device went away or stopped delivering.
    Disconnected(String),
    Failed(String),
    Stopped,
}
