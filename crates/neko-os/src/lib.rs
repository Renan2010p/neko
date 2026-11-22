//! # neko-os
//!
//! The operating-system layer, after Godot's `OS` (core/os/os.h) and
//! `FileAccess`/`DirAccess` (core/io). The engine never calls `std::fs`,
//! `std::time` or `std::env` directly: it goes through [`Os`], so a port to a
//! new platform (a console, an embedded device) only implements these traits.
//!
//! [`StdOs`] (feature `std-backend`, on by default) implements them on top of
//! Rust's standard library, which covers desktop, mobile, BSDs and Haiku.
//!
//! Engine paths are virtual, like Godot's: `res://` is the game's data
//! (read-only on most platforms once exported) and `user://` is the save
//! directory. [`Paths`] maps them to the paths a [`FileSystem`] understands.

use std::fmt;

mod fs;
pub use fs::*;

#[cfg(feature = "std-backend")]
mod std_os;
#[cfg(feature = "std-backend")]
pub use std_os::StdOs;

#[derive(Debug, thiserror::Error)]
pub enum OsError {
    #[error("not supported on this platform: {0}")]
    Unsupported(&'static str),
    #[error("{0}")]
    Other(String),
}

/// Severity of a log line (Godot's print / print_error / push_warning).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Warning,
    Error,
}

/// Calendar date and time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DateTime {
    pub year: i32,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

/// Everything the engine needs from the operating system.
///
/// Implementations must be cheap to call from any thread.
pub trait Os: Send + Sync {
    /// Platform name, as Godot's `OS.get_name()`: "Linux", "Windows", "macOS",
    /// "Android", "iOS", "Web", "FreeBSD", "Haiku", or a console's name.
    fn name(&self) -> &str;

    /// Microseconds since the engine started, monotonic.
    fn ticks_usec(&self) -> u64;

    /// Block the calling thread.
    fn delay_usec(&self, usec: u64);

    /// Seconds since 1970-01-01 UTC.
    fn unix_time(&self) -> f64;

    /// Cryptographically secure random bytes (Godot's `OS::get_entropy`).
    fn entropy(&self, buf: &mut [u8]) -> Result<(), OsError>;

    /// Number of logical CPU cores the game may use.
    fn processor_count(&self) -> usize {
        1
    }

    /// Run `job` on a new thread. Platforms without threads return
    /// [`OsError::Unsupported`] and the engine falls back to running inline.
    fn spawn(&self, name: &str, job: Box<dyn FnOnce() + Send>) -> Result<(), OsError>;

    /// Write a line to the platform's log (stdout, logcat, the console's TTY...).
    fn log(&self, level: LogLevel, text: &str);

    /// An environment variable, where the platform has them.
    fn env(&self, _key: &str) -> Option<String> {
        None
    }

    /// Command-line arguments, without the executable name.
    fn args(&self) -> Vec<String> {
        Vec::new()
    }

    /// The platform's preferred locale, like `"pt_BR"`.
    fn locale(&self) -> String {
        "en".into()
    }

    /// Where `user://` lives for a game named `app` (the save directory).
    fn user_data_dir(&self, app: &str) -> Option<String>;

    /// The file system.
    fn fs(&self) -> &dyn FileSystem;
}

/// Maps Godot-style virtual paths to [`FileSystem`] paths.
#[derive(Clone, Debug)]
pub struct Paths {
    /// Directory (or package root) that `res://` points to.
    pub res: String,
    /// Directory that `user://` points to.
    pub user: String,
}

impl Paths {
    /// `res://scenes/main.scn` -> `<res>/scenes/main.scn`. Other paths are
    /// returned unchanged.
    pub fn resolve(&self, path: &str) -> String {
        let join = |root: &str, rest: &str| {
            if root.is_empty() {
                rest.to_string()
            } else {
                format!("{}/{}", root.trim_end_matches('/'), rest)
            }
        };
        if let Some(rest) = path.strip_prefix("res://") {
            join(&self.res, rest)
        } else if let Some(rest) = path.strip_prefix("user://") {
            join(&self.user, rest)
        } else {
            path.to_string()
        }
    }
}

impl fmt::Display for DateTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02} {:02}:{:02}:{:02}", self.year, self.month, self.day, self.hour, self.minute, self.second)
    }
}

/// Convert Unix seconds to a UTC calendar date (no time zone database needed).
pub fn utc_datetime(unix: f64) -> DateTime {
    let secs = unix.floor() as i64;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
    let year = (yoe + era * 400 + i64::from(month <= 2)) as i32;
    DateTime { year, month, day, hour: (rem / 3600) as u8, minute: (rem % 3600 / 60) as u8, second: (rem % 60) as u8 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn virtual_paths() {
        let p = Paths { res: "/games/demo/".into(), user: "/home/a/.local/share/demo".into() };
        assert_eq!(p.resolve("res://scenes/main.scn"), "/games/demo/scenes/main.scn");
        assert_eq!(p.resolve("user://save.dat"), "/home/a/.local/share/demo/save.dat");
        assert_eq!(p.resolve("/etc/hosts"), "/etc/hosts");
    }

    #[test]
    fn calendar() {
        assert_eq!(utc_datetime(0.0).to_string(), "1970-01-01 00:00:00");
        assert_eq!(utc_datetime(1_795_000_000.0).to_string(), "2026-11-18 11:06:40");
        assert_eq!(utc_datetime(951_782_400.0).to_string(), "2000-02-29 00:00:00");
    }
}
