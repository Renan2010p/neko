//! Files and directories (Godot's `FileAccess` and `DirAccess`).

use std::io;

/// How to open a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenMode {
    Read,
    /// Create or truncate.
    Write,
    /// Read and write an existing file.
    ReadWrite,
    /// Create if missing, write at the end.
    Append,
}

/// An open file. Reads and writes use `std::io`'s traits so the rest of the
/// engine (decoders, serializers) works on any platform's files.
pub trait File: io::Read + io::Write + io::Seek + Send {
    /// Size in bytes.
    fn len(&mut self) -> io::Result<u64>;
    fn is_empty(&mut self) -> io::Result<bool> {
        Ok(self.len()? == 0)
    }
}

/// One entry of a directory listing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
}

/// File metadata.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metadata {
    pub is_dir: bool,
    pub len: u64,
    /// Seconds since the Unix epoch, when the platform tracks it.
    pub modified: Option<f64>,
}

/// A file system. Paths are the platform's own (see [`crate::Paths`]).
pub trait FileSystem: Send + Sync {
    fn open(&self, path: &str, mode: OpenMode) -> io::Result<Box<dyn File>>;
    fn metadata(&self, path: &str) -> io::Result<Metadata>;
    /// Entries of a directory, in no particular order, without `.` and `..`.
    fn read_dir(&self, path: &str) -> io::Result<Vec<DirEntry>>;
    fn create_dir_all(&self, path: &str) -> io::Result<()>;
    /// Remove a file or an empty directory.
    fn remove(&self, path: &str) -> io::Result<()>;
    fn rename(&self, from: &str, to: &str) -> io::Result<()>;

    fn exists(&self, path: &str) -> bool {
        self.metadata(path).is_ok()
    }

    /// Read a whole file.
    fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        let mut f = self.open(path, OpenMode::Read)?;
        let mut out = Vec::new();
        io::Read::read_to_end(&mut f, &mut out)?;
        Ok(out)
    }

    /// Replace a file's contents.
    fn write(&self, path: &str, data: &[u8]) -> io::Result<()> {
        let mut f = self.open(path, OpenMode::Write)?;
        io::Write::write_all(&mut f, data)?;
        io::Write::flush(&mut f)
    }
}
