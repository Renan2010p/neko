//! # neko-project
//!
//! Everything about Neko *projects*, independent of any UI:
//!
//! - [`catalog`]: available backends, target platforms and their support matrix.
//! - [`config`]: the `project.neko` file inside each project.
//! - [`migrate`]: converting Neko 0.1 projects (`neko.toml`, TOML `.scn`).
//! - [`registry`]: the list of projects the project manager knows about.
//! - [`scaffold`]: creating a new project on disk.

pub mod catalog;
pub mod config;
pub mod migrate;
pub mod registry;
pub mod scaffold;

pub use catalog::*;
pub use config::ProjectConfig;
pub use registry::{ProjectEntry, Registry};
pub use scaffold::{NewProject, create_project};

#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("I/O error at {path}: {source}")]
    Io { path: std::path::PathBuf, source: std::io::Error },
    #[error("invalid project file {path}: {message}")]
    Parse { path: std::path::PathBuf, message: String },
    #[error("{0}")]
    Invalid(String),
}

pub(crate) fn io_err(path: &std::path::Path) -> impl FnOnce(std::io::Error) -> ProjectError + '_ {
    move |source| ProjectError::Io { path: path.to_path_buf(), source }
}

/// True if `dir` holds a project Neko can open: `project.neko`, a Godot
/// `project.godot`, or Neko 0.1's `neko.toml`.
pub fn is_project_dir(dir: &std::path::Path) -> bool {
    [config::FILE_NAME, migrate::GODOT_FILE_NAME, config::LEGACY_FILE_NAME].iter().any(|f| dir.join(f).is_file())
}

/// Parse the contents of a `project.neko` (or a legacy `neko.toml`).
pub fn config_from_str(text: &str) -> Result<ProjectConfig, String> {
    ProjectConfig::parse(text)
}

/// Find the file with `uid://...` in a project: scene and resource headers,
/// `*.import` files and `*.uid` files (Godot's sources of UIDs). Returns the
/// path relative to `dir`, without `res://`.
pub fn resolve_uid(dir: &std::path::Path, uid: &str) -> Option<String> {
    fn walk(root: &std::path::Path, dir: &std::path::Path, uid: &str, depth: usize) -> Option<String> {
        if depth > 12 {
            return None;
        }
        for e in std::fs::read_dir(dir).ok()?.flatten() {
            let path = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || name == "target" {
                continue;
            }
            if path.is_dir() {
                if let Some(found) = walk(root, &path, uid, depth + 1) {
                    return Some(found);
                }
                continue;
            }
            let rel = |p: &std::path::Path| p.strip_prefix(root).ok().map(|r| r.to_string_lossy().replace('\\', "/"));
            match path.extension().and_then(|x| x.to_str()) {
                Some("tscn" | "tres") => {
                    let head = std::fs::read_to_string(&path).ok()?.lines().next().unwrap_or("").to_string();
                    if head.contains(&format!("uid=\"{uid}\"")) {
                        return rel(&path);
                    }
                }
                Some("import") if std::fs::read_to_string(&path).unwrap_or_default().contains(&format!("uid=\"{uid}\"")) => {
                    return rel(&path.with_extension(""));
                }
                Some("uid") if std::fs::read_to_string(&path).unwrap_or_default().trim() == uid => {
                    return rel(&path.with_extension(""));
                }
                _ => {}
            }
        }
        None
    }
    walk(dir, dir, uid, 0)
}
