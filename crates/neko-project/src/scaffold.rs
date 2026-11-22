//! Creating a new project on disk.

use std::path::{Path, PathBuf};

use crate::catalog::{BackendSet, TargetPlatform, Template};
use crate::config::{ProjectConfig, ProjectSection, TargetsSection};
use crate::{ProjectError, io_err};

pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Debug)]
pub struct NewProject {
    pub name: String,
    /// Folder to create. Must not exist, or be empty.
    pub path: PathBuf,
    pub template: Template,
    pub backends: BackendSet,
    pub targets: Vec<TargetPlatform>,
    pub git: bool,
    /// Path to the Neko engine checkout. When set, the generated `Cargo.toml`
    /// depends on it by path instead of by crates.io version.
    pub engine_path: Option<PathBuf>,
}

/// `"My Cool Game!"` -> `"my-cool-game"`.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        let c = fold_accent(c).to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() { "project".into() } else { out }
}

fn fold_accent(c: char) -> char {
    match c {
        'á' | 'à' | 'â' | 'ã' | 'ä' | 'Á' | 'À' | 'Â' | 'Ã' | 'Ä' => 'a',
        'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => 'e',
        'í' | 'ì' | 'î' | 'ï' | 'Í' | 'Ì' | 'Î' | 'Ï' => 'i',
        'ó' | 'ò' | 'ô' | 'õ' | 'ö' | 'Ó' | 'Ò' | 'Ô' | 'Õ' | 'Ö' => 'o',
        'ú' | 'ù' | 'û' | 'ü' | 'Ú' | 'Ù' | 'Û' | 'Ü' => 'u',
        'ç' | 'Ç' => 'c',
        'ñ' | 'Ñ' => 'n',
        other => other,
    }
}

impl NewProject {
    pub fn crate_name(&self) -> String {
        let s = slug(&self.name).replace('-', "_");
        if s.starts_with(|c: char| c.is_ascii_digit()) { format!("game_{s}") } else { s }
    }

    /// Problems that block creation, in display order.
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("Give the project a name.".into());
        }
        if self.path.as_os_str().is_empty() {
            return Err("Choose a folder for the project.".into());
        }
        if self.path.is_file() {
            return Err("That path is a file. Choose a folder.".into());
        }
        if self.path.is_dir() && std::fs::read_dir(&self.path).map(|mut d| d.next().is_some()).unwrap_or(true) {
            return Err("That folder is not empty. Choose a new or empty folder.".into());
        }
        if self.targets.is_empty() {
            return Err("Select at least one target platform.".into());
        }
        Ok(())
    }

    pub fn config(&self) -> ProjectConfig {
        ProjectConfig {
            project: ProjectSection {
                name: self.name.trim().to_string(),
                template: self.template,
                main_scene: "scenes/main.tscn".into(),
                engine: ENGINE_VERSION.into(),
            },
            backends: self.backends,
            targets: TargetsSection { platforms: self.targets.clone() },
            boot_splash: Default::default(),
            file: Default::default(),
        }
    }

    pub fn cargo_toml(&self) -> String {
        let features = self.backends.features(self.template);
        let dep = match &self.engine_path {
            Some(p) => format!("path = {:?}", p.join("crates").join("neko").display().to_string()),
            None => format!("version = \"{ENGINE_VERSION}\""),
        };
        let mut s = format!(
            "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nneko = {{ {dep}, default-features = false, features = [\n",
            self.crate_name()
        );
        for f in features {
            s += &format!("    \"{f}\",\n");
        }
        s += "] }\n";
        s
    }

    pub fn main_rs(&self) -> String {
        format!(
            "//! {name}\n//!\n//! Backends and targets live in `project.neko`; switch backends by\n//! editing it and the `neko` features in `Cargo.toml`.\n\nfn main() {{\n    neko::run(include_str!(\"../project.neko\"));\n}}\n",
            name = self.name.trim()
        )
    }

    pub fn main_scene(&self) -> String {
        let mut scene = neko_core::Scene::new(self.template.root_node(), "Main").expect("template roots are valid classes");
        neko_core::format::to_string(&mut scene)
    }

    /// Files that will be written, relative to the project folder.
    pub fn files(&self) -> Vec<(PathBuf, String)> {
        let mut files = vec![
            (PathBuf::from("Cargo.toml"), self.cargo_toml()),
            (PathBuf::from(crate::config::FILE_NAME), self.config().to_text()),
            (PathBuf::from("src/main.rs"), self.main_rs()),
            (PathBuf::from("assets/.keep"), String::new()),
        ];
        // An empty template starts with no scene, like Godot: the editor offers "Create Root Node".
        if self.template != Template::Empty {
            files.push((PathBuf::from("scenes/main.tscn"), self.main_scene()));
        }
        if self.git {
            files.push((PathBuf::from(".gitignore"), "/target\n/.neko/\n/.godot/\n".into()));
        }
        files
    }
}

/// Write the project to disk. Runs `git init` when requested (a missing
/// `git` binary is not an error).
pub fn create_project(p: &NewProject) -> Result<PathBuf, ProjectError> {
    p.validate().map_err(ProjectError::Invalid)?;
    for (rel, content) in p.files() {
        let full = p.path.join(&rel);
        if let Some(dir) = full.parent() {
            std::fs::create_dir_all(dir).map_err(io_err(dir))?;
        }
        std::fs::write(&full, content).map_err(io_err(&full))?;
    }
    if p.git {
        let _ = std::process::Command::new("git").arg("init").arg("-q").current_dir(&p.path).status();
    }
    Ok(p.path.clone())
}

/// Default parent folder for new projects (`~/Projects` style).
pub fn default_projects_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    for name in ["Projects", "Projetos", "projects"] {
        let p = home.join(name);
        if p.is_dir() {
            return p;
        }
    }
    home.join("Projects")
}

pub fn exists_and_not_empty(path: &Path) -> bool {
    path.is_dir() && std::fs::read_dir(path).map(|mut d| d.next().is_some()).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Registry;

    fn sample(dir: PathBuf) -> NewProject {
        NewProject {
            name: "Gato Plataforma 2".into(),
            path: dir,
            template: Template::TwoD,
            backends: BackendSet::default(),
            targets: vec![TargetPlatform::Linux],
            git: false,
            engine_path: None,
        }
    }

    #[test]
    fn slugs() {
        assert_eq!(slug("Gato Plataforma!"), "gato-plataforma");
        assert_eq!(slug("Ação 3D"), "acao-3d");
        assert_eq!(slug("  "), "project");
        assert_eq!(sample(PathBuf::new()).crate_name(), "gato_plataforma_2");
    }

    #[test]
    fn create_scan_and_registry() {
        let root = std::env::temp_dir().join(format!("neko-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let p = sample(root.join("games").join("gato"));
        create_project(&p).unwrap();
        let cfg = ProjectConfig::load(&p.path).unwrap();
        assert_eq!(cfg.project.name, "Gato Plataforma 2");
        assert!(std::fs::read_to_string(p.path.join("Cargo.toml")).unwrap().contains("\"gamepad-sdl3\""));
        assert!(p.validate().is_err(), "non-empty folder must be rejected");

        let mut reg = Registry::load_from(root.join("reg.toml")).unwrap();
        assert_eq!(reg.scan(&root, 3), 1);
        assert_eq!(reg.scan(&root, 3), 0);
        reg.save().unwrap();
        let reg2 = Registry::load_from(root.join("reg.toml")).unwrap();
        assert_eq!(reg2.projects.len(), 1);
        std::fs::remove_dir_all(&root).unwrap();
        let mut reg3 = reg2.clone();
        assert_eq!(reg3.remove_missing(), 1);
    }
}
