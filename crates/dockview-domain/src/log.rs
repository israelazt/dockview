//! A small diagnostic log: what the program wants to leave written down (a
//! device that failed, a recovery, a setting that could not be saved), without
//! saying where it goes. The program installs one sink at start-up (a file,
//! and the console when there is one); until then, and in tests, the messages
//! go nowhere.
//!
//! Use the macros: `log_info!("started {device}")`, `log_warn!`, `log_error!`.

use std::fmt;
use std::sync::OnceLock;

/// How serious a message is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Info,
    Warn,
    Error,
}

impl Level {
    /// The word written in the log: `INFO`, `WARN` or `ERROR`.
    pub fn label(self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        }
    }
}

/// Where the messages end up. It may be called from any thread.
pub trait LogSink: Send + Sync {
    fn write(&self, level: Level, message: &str);
}

static SINK: OnceLock<Box<dyn LogSink>> = OnceLock::new();

/// Installs the sink. Only the first call counts; it says whether this one did.
pub fn set_sink(sink: Box<dyn LogSink>) -> bool {
    SINK.set(sink).is_ok()
}

/// Sends a message to `sink`, if there is one. The macros use it with the
/// installed sink.
pub fn emit_to(sink: Option<&dyn LogSink>, level: Level, message: fmt::Arguments<'_>) {
    if let Some(sink) = sink {
        sink.write(level, &message.to_string());
    }
}

/// Sends a message to the installed sink (see [`set_sink`]).
#[doc(hidden)]
pub fn emit(level: Level, message: fmt::Arguments<'_>) {
    emit_to(SINK.get().map(AsRef::as_ref), level, message);
}

/// Writes an `INFO` line to the log.
#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => { $crate::log::emit($crate::log::Level::Info, format_args!($($arg)*)) };
}

/// Writes a `WARN` line to the log.
#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => { $crate::log::emit($crate::log::Level::Warn, format_args!($($arg)*)) };
}

/// Writes an `ERROR` line to the log.
#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => { $crate::log::emit($crate::log::Level::Error, format_args!($($arg)*)) };
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    #[derive(Default)]
    struct Memory(Arc<Mutex<Vec<(Level, String)>>>);

    impl LogSink for Memory {
        fn write(&self, level: Level, message: &str) {
            self.0.lock().unwrap().push((level, message.to_owned()));
        }
    }

    #[test]
    fn a_sink_receives_the_message_formatted_with_its_level() {
        let memory = Memory::default();
        emit_to(
            Some(&memory),
            Level::Warn,
            format_args!("{} failed: {}", "mic", 7),
        );
        emit_to(Some(&memory), Level::Info, format_args!("ok"));
        let lines = memory.0.lock().unwrap();
        assert_eq!(lines[0], (Level::Warn, "mic failed: 7".to_string()));
        assert_eq!(lines[1], (Level::Info, "ok".to_string()));
    }

    #[test]
    fn without_a_sink_messages_go_nowhere_and_nothing_breaks() {
        emit_to(None, Level::Error, format_args!("lost"));
        // The macros expand and run with no sink installed.
        crate::log_info!("started {}", 1);
        crate::log_warn!("careful");
        crate::log_error!("failed: {}", "x");
    }

    #[test]
    fn levels_are_ordered_and_have_their_words() {
        assert!(Level::Info < Level::Warn && Level::Warn < Level::Error);
        assert_eq!(
            [Level::Info, Level::Warn, Level::Error].map(Level::label),
            ["INFO", "WARN", "ERROR"]
        );
    }
}
