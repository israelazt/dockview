//! The log on disk: `%APPDATA%\Dockview\dockview.log`. It is the real
//! implementation of the domain's [`LogSink`]; it also mirrors every line to
//! the console, which is there only when the program was started from one.
//!
//! The file turns over at 1 MB: the full one becomes `dockview.log.old` (and
//! replaces an older copy), so the log never grows without limit and the last
//! run or two are always there.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use dockview_domain::log::{set_sink, Level, LogSink};

/// The size at which the file turns over.
pub const MAX_LOG_BYTES: u64 = 1_000_000;

/// A moment on the local clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timestamp {
    pub year: u16,
    pub month: u16,
    pub day: u16,
    pub hour: u16,
    pub minute: u16,
    pub second: u16,
    pub millis: u16,
}

impl Timestamp {
    /// The local time now.
    pub fn now() -> Self {
        // SAFETY: a plain query with no arguments.
        let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
        Self {
            year: t.wYear,
            month: t.wMonth,
            day: t.wDay,
            hour: t.wHour,
            minute: t.wMinute,
            second: t.wSecond,
            millis: t.wMilliseconds,
        }
    }
}

/// One line of the log: `2026-11-13 13:20:01.123 WARN  message`, ending in a
/// newline. A message of several lines is kept on one, so every entry starts
/// with its time.
pub fn format_line(at: Timestamp, level: Level, message: &str) -> String {
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03} {:<5} {}\n",
        at.year,
        at.month,
        at.day,
        at.hour,
        at.minute,
        at.second,
        at.millis,
        level.label(),
        message.trim_end().replace('\n', " | ")
    )
}

/// An append-only text file that turns over when it gets big.
pub struct LogFile {
    path: PathBuf,
    max_bytes: u64,
    file: File,
    written: u64,
}

impl LogFile {
    /// Opens (or creates) the file at `path`, appending. A file that is
    /// already over `max_bytes` turns over first.
    pub fn open(path: &Path, max_bytes: u64) -> io::Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let size = fs::metadata(path).map_or(0, |m| m.len());
        if size >= max_bytes {
            Self::turn_over(path)?;
        }
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        let written = file.metadata()?.len();
        Ok(Self {
            path: path.to_owned(),
            max_bytes,
            file,
            written,
        })
    }

    /// Appends `line`, turning the file over first if it would not fit.
    pub fn write_line(&mut self, line: &str) -> io::Result<()> {
        if self.written > 0 && self.written + line.len() as u64 > self.max_bytes {
            Self::turn_over(&self.path)?;
            self.file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)?;
            self.written = 0;
        }
        self.file.write_all(line.as_bytes())?;
        self.written += line.len() as u64;
        Ok(())
    }

    /// Moves the full file to `<name>.old`, replacing an older copy.
    fn turn_over(path: &Path) -> io::Result<()> {
        let old = old_path(path);
        let _ = fs::remove_file(&old);
        fs::rename(path, old)
    }
}

/// Where the full file is kept: the same name followed by `.old`.
pub fn old_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".old");
    PathBuf::from(name)
}

/// The log file: `%APPDATA%\Dockview\dockview.log`.
pub fn default_log_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|dir| PathBuf::from(dir).join("Dockview").join("dockview.log"))
}

/// The sink that writes the file and mirrors to the console.
pub struct FileSink {
    file: Mutex<Option<LogFile>>,
    console: bool,
}

impl FileSink {
    /// A sink writing to `path`. When the file cannot be opened the lines still
    /// reach the console.
    pub fn new(path: &Path, console: bool) -> Self {
        Self {
            file: Mutex::new(LogFile::open(path, MAX_LOG_BYTES).ok()),
            console,
        }
    }
}

impl LogSink for FileSink {
    fn write(&self, level: Level, message: &str) {
        let line = format_line(Timestamp::now(), level, message);
        if self.console {
            // Ignored by the standard library when there is no console.
            eprint!("{line}");
        }
        if let Some(file) = self.file.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            let _ = file.write_line(&line);
        }
    }
}

/// Installs the file sink as the program's log. Returns whether it was
/// installed (it is not when there is no `%APPDATA%` or a sink was already set).
pub fn install_default(console: bool) -> bool {
    default_log_path().is_some_and(|path| set_sink(Box::new(FileSink::new(&path, console))))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_log(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dockview-log-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir.join("dockview.log")
    }

    fn at() -> Timestamp {
        Timestamp {
            year: 2026,
            month: 11,
            day: 3,
            hour: 9,
            minute: 5,
            second: 7,
            millis: 42,
        }
    }

    #[test]
    fn a_line_starts_with_its_time_and_level_and_stays_on_one_line() {
        assert_eq!(
            format_line(at(), Level::Warn, "mic lost"),
            "2026-11-03 09:05:07.042 WARN  mic lost\n"
        );
        assert_eq!(
            format_line(at(), Level::Error, "first\nsecond\n"),
            "2026-11-03 09:05:07.042 ERROR first | second\n"
        );
    }

    #[test]
    fn lines_are_appended_and_a_new_run_continues_the_same_file() {
        let path = temp_log("append");
        let mut log = LogFile::open(&path, 10_000).unwrap();
        log.write_line("one\n").unwrap();
        drop(log);
        let mut again = LogFile::open(&path, 10_000).unwrap();
        again.write_line("two\n").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "one\ntwo\n");
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn the_file_turns_over_when_it_would_pass_the_limit() {
        let path = temp_log("rotate");
        let mut log = LogFile::open(&path, 30).unwrap();
        for line in ["aaaaaaaaaaaaaaaaaaaa\n", "bbbbbbbbbbbbbbbbbbbb\n", "cccc\n"] {
            log.write_line(line).unwrap();
        }
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "bbbbbbbbbbbbbbbbbbbb\ncccc\n"
        );
        assert_eq!(
            fs::read_to_string(old_path(&path)).unwrap(),
            "aaaaaaaaaaaaaaaaaaaa\n"
        );
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_big_old_file_turns_over_when_opened_and_the_older_copy_is_replaced() {
        let path = temp_log("big");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "x".repeat(100)).unwrap();
        fs::write(old_path(&path), "very old").unwrap();
        let mut log = LogFile::open(&path, 50).unwrap();
        log.write_line("fresh\n").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "fresh\n");
        assert_eq!(
            fs::read_to_string(old_path(&path)).unwrap(),
            "x".repeat(100)
        );
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn the_sink_writes_a_timestamped_line_to_the_file() {
        let path = temp_log("sink");
        let sink = FileSink::new(&path, false);
        sink.write(Level::Info, "started");
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains(" INFO  started\n"), "{text:?}");
        assert!(text.starts_with("20"), "{text:?}");
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }
}
