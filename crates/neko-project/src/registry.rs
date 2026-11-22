//! The project manager's list of known projects.
//!
//! Stored in `<data dir>/neko/projects.toml` (e.g. `~/.local/share/neko` on
//! Linux, `%APPDATA%\neko` on Windows).

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::config::{FILE_NAME, ProjectConfig};
use crate::{ProjectError, io_err};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectEntry {
    pub path: PathBuf,
    #[serde(default)]
    pub favorite: bool,
    /// Unix seconds.
    #[serde(default)]
    pub last_opened: u64,
    #[serde(default)]
    pub tags: Vec<String>,
}

impl ProjectEntry {
    pub fn new(path: PathBuf) -> Self {
        Self { path, favorite: false, last_opened: now(), tags: Vec::new() }
    }

    pub fn exists(&self) -> bool {
        crate::is_project_dir(&self.path)
    }

    pub fn load_config(&self) -> Result<ProjectConfig, ProjectError> {
        ProjectConfig::load(&self.path)
    }

    pub fn touch(&mut self) {
        self.last_opened = now();
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Registry {
    #[serde(default, rename = "project")]
    pub projects: Vec<ProjectEntry>,
    #[serde(skip)]
    file: Option<PathBuf>,
}

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

impl Registry {
    /// Default location of the registry file.
    pub fn default_file() -> Option<PathBuf> {
        dirs::data_dir().map(|d| d.join("neko").join("projects.toml"))
    }

    /// Load from `file`; a missing file gives an empty registry.
    pub fn load_from(file: PathBuf) -> Result<Self, ProjectError> {
        let mut reg = match std::fs::read_to_string(&file) {
            Ok(text) => toml::from_str::<Registry>(&text)
                .map_err(|e| ProjectError::Parse { path: file.clone(), message: e.to_string() })?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Registry::default(),
            Err(e) => return Err(ProjectError::Io { path: file, source: e }),
        };
        reg.file = Some(file);
        Ok(reg)
    }

    pub fn load_default() -> Result<Self, ProjectError> {
        let file = Self::default_file().ok_or_else(|| ProjectError::Invalid("no data directory on this system".into()))?;
        Self::load_from(file)
    }

    pub fn file(&self) -> Option<&Path> {
        self.file.as_deref()
    }

    pub fn save(&self) -> Result<(), ProjectError> {
        let Some(file) = &self.file else { return Ok(()) };
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir).map_err(io_err(dir))?;
        }
        let text = toml::to_string_pretty(self).expect("Registry always serializes");
        std::fs::write(file, text).map_err(io_err(file))
    }

    pub fn find(&self, path: &Path) -> Option<usize> {
        self.projects.iter().position(|p| p.path == path)
    }

    /// Add a project folder (must contain `project.neko`, or a Neko 0.1 `neko.toml`). Returns its index.
    pub fn add(&mut self, path: PathBuf) -> Result<usize, ProjectError> {
        if !crate::is_project_dir(&path) {
            return Err(ProjectError::Invalid(format!("{} has no {FILE_NAME}", path.display())));
        }
        if let Some(i) = self.find(&path) {
            self.projects[i].touch();
            return Ok(i);
        }
        self.projects.push(ProjectEntry::new(path));
        Ok(self.projects.len() - 1)
    }

    pub fn remove(&mut self, index: usize) -> ProjectEntry {
        self.projects.remove(index)
    }

    /// Drop entries whose folder no longer has a project file. Returns how many were removed.
    pub fn remove_missing(&mut self) -> usize {
        let before = self.projects.len();
        self.projects.retain(|p| p.exists());
        before - self.projects.len()
    }

    /// Find project folders under `root` (up to `max_depth` levels) and add them.
    /// Returns how many new projects were added.
    pub fn scan(&mut self, root: &Path, max_depth: usize) -> usize {
        let mut found = Vec::new();
        scan_dir(root, max_depth, &mut found);
        let mut added = 0;
        for dir in found {
            if self.find(&dir).is_none() && self.add(dir).is_ok() {
                added += 1;
            }
        }
        added
    }
}

fn scan_dir(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if crate::is_project_dir(dir) {
        out.push(dir.to_path_buf());
        return;
    }
    if depth == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let name = e.file_name();
        let hidden = name.to_string_lossy().starts_with('.');
        if !hidden && name != "target" && name != "node_modules" && e.file_type().is_ok_and(|t| t.is_dir()) {
            scan_dir(&e.path(), depth - 1, out);
        }
    }
}
