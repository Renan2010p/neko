//! [`Os`] on top of Rust's standard library.

use std::io;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::{DirEntry, File, FileSystem, LogLevel, Metadata, OpenMode, Os, OsError};

/// The default OS backend: every target where Rust's `std` works.
pub struct StdOs {
    start: Instant,
    fs: StdFs,
}

impl Default for StdOs {
    fn default() -> Self {
        Self { start: Instant::now(), fs: StdFs }
    }
}

impl StdOs {
    pub fn new() -> Self {
        Self::default()
    }
}

fn os_name() -> &'static str {
    match std::env::consts::OS {
        "linux" => "Linux",
        "windows" => "Windows",
        "macos" => "macOS",
        "ios" => "iOS",
        "android" => "Android",
        "freebsd" => "FreeBSD",
        "netbsd" => "NetBSD",
        "openbsd" => "OpenBSD",
        "dragonfly" => "DragonFly",
        "haiku" => "Haiku",
        "emscripten" | "unknown" => "Web",
        other => other,
    }
}

impl Os for StdOs {
    fn name(&self) -> &str {
        os_name()
    }

    fn ticks_usec(&self) -> u64 {
        self.start.elapsed().as_micros() as u64
    }

    fn delay_usec(&self, usec: u64) {
        std::thread::sleep(std::time::Duration::from_micros(usec));
    }

    fn unix_time(&self) -> f64 {
        SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
    }

    fn entropy(&self, buf: &mut [u8]) -> Result<(), OsError> {
        getrandom::fill(buf).map_err(|e| OsError::Other(e.to_string()))
    }

    fn processor_count(&self) -> usize {
        std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1)
    }

    fn spawn(&self, name: &str, job: Box<dyn FnOnce() + Send>) -> Result<(), OsError> {
        std::thread::Builder::new().name(name.to_string()).spawn(job).map(|_| ()).map_err(|e| OsError::Other(e.to_string()))
    }

    fn log(&self, level: LogLevel, text: &str) {
        match level {
            LogLevel::Info => println!("{text}"),
            LogLevel::Warning => eprintln!("WARNING: {text}"),
            LogLevel::Error => eprintln!("ERROR: {text}"),
        }
    }

    fn env(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }

    fn args(&self) -> Vec<String> {
        std::env::args().skip(1).collect()
    }

    fn locale(&self) -> String {
        // POSIX locale variables, e.g. "pt_BR.UTF-8" -> "pt_BR".
        ["LC_ALL", "LC_MESSAGES", "LANG"]
            .iter()
            .filter_map(|k| std::env::var(k).ok())
            .find(|v| !v.is_empty() && v != "C" && v != "POSIX")
            .map(|v| v.split(['.', '@']).next().unwrap_or("en").to_string())
            .unwrap_or_else(|| "en".into())
    }

    fn user_data_dir(&self, app: &str) -> Option<String> {
        // Godot: ~/.local/share/godot/app_userdata/<name> on Linux, %APPDATA% on Windows.
        dirs::data_dir().map(|d| d.join("neko").join("app_userdata").join(app).to_string_lossy().into_owned())
    }

    fn fs(&self) -> &dyn FileSystem {
        &self.fs
    }
}

/// `std::fs`.
struct StdFs;

impl File for std::fs::File {
    fn len(&mut self) -> io::Result<u64> {
        Ok(self.metadata()?.len())
    }
}

impl FileSystem for StdFs {
    fn open(&self, path: &str, mode: OpenMode) -> io::Result<Box<dyn File>> {
        let mut o = std::fs::OpenOptions::new();
        match mode {
            OpenMode::Read => o.read(true),
            OpenMode::Write => o.write(true).create(true).truncate(true),
            OpenMode::ReadWrite => o.read(true).write(true),
            OpenMode::Append => o.append(true).create(true),
        };
        Ok(Box::new(o.open(path)?))
    }

    fn metadata(&self, path: &str) -> io::Result<Metadata> {
        let m = std::fs::metadata(path)?;
        let modified = m.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs_f64());
        Ok(Metadata { is_dir: m.is_dir(), len: m.len(), modified })
    }

    fn read_dir(&self, path: &str) -> io::Result<Vec<DirEntry>> {
        std::fs::read_dir(path)?
            .map(|e| {
                let e = e?;
                Ok(DirEntry { name: e.file_name().to_string_lossy().into_owned(), is_dir: e.file_type()?.is_dir() })
            })
            .collect()
    }

    fn create_dir_all(&self, path: &str) -> io::Result<()> {
        std::fs::create_dir_all(path)
    }

    fn remove(&self, path: &str) -> io::Result<()> {
        if std::fs::metadata(path)?.is_dir() { std::fs::remove_dir(path) } else { std::fs::remove_file(path) }
    }

    fn rename(&self, from: &str, to: &str) -> io::Result<()> {
        std::fs::rename(from, to)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn std_backend_round_trip() {
        let os = StdOs::new();
        let dir = std::env::temp_dir().join(format!("neko-os-test-{}", std::process::id()));
        let dir = dir.to_string_lossy().into_owned();
        let fs = os.fs();
        fs.create_dir_all(&dir).unwrap();
        let file = format!("{dir}/save.dat");
        fs.write(&file, b"neko").unwrap();
        assert_eq!(fs.read(&file).unwrap(), b"neko");
        assert_eq!(fs.metadata(&file).unwrap().len, 4);
        assert_eq!(fs.read_dir(&dir).unwrap(), vec![DirEntry { name: "save.dat".into(), is_dir: false }]);
        fs.remove(&file).unwrap();
        fs.remove(&dir).unwrap();
        assert!(!fs.exists(&dir));

        let mut a = [0u8; 16];
        os.entropy(&mut a).unwrap();
        assert_ne!(a, [0; 16]);
        let t = os.ticks_usec();
        os.delay_usec(1000);
        assert!(os.ticks_usec() >= t + 1000);
    }
}
