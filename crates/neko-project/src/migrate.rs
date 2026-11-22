//! Getting projects ready to open in Neko:
//!
//! - Godot projects (`project.godot`): `project.neko` is created from it
//!   (every Godot setting is kept), plus the small Cargo crate that runs the
//!   game. `project.godot` is not touched, so the project still opens in Godot.
//! - Neko 0.1 projects: `neko.toml` -> `project.neko`, TOML `.scn` -> `.tscn`.

use std::path::{Path, PathBuf};

use crate::config::{LEGACY_FILE_NAME, ProjectConfig};
use crate::{ProjectError, io_err};

/// Godot's project file.
pub const GODOT_FILE_NAME: &str = "project.godot";

/// Convert whatever needs converting before the editor opens `dir`.
/// Returns one line per change (empty when nothing was needed).
pub fn prepare_project(dir: &Path, engine_path: Option<&Path>) -> Result<Vec<String>, ProjectError> {
    let mut log = Vec::new();
    if !dir.join(crate::config::FILE_NAME).is_file() && dir.join(GODOT_FILE_NAME).is_file() {
        log.extend(import_godot_project(dir, engine_path)?);
    }
    if needs_migration(dir) {
        log.extend(migrate_project(dir)?);
    }
    Ok(log)
}

/// Make a Godot project runnable by Neko.
pub fn import_godot_project(dir: &Path, engine_path: Option<&Path>) -> Result<Vec<String>, ProjectError> {
    let gp = dir.join(GODOT_FILE_NAME);
    let text = std::fs::read_to_string(&gp).map_err(io_err(&gp))?;
    let mut config = ProjectConfig::parse(&text).map_err(|message| ProjectError::Parse { path: gp.clone(), message })?;
    if config.project.main_scene.starts_with("uid://")
        && let Some(p) = crate::resolve_uid(dir, &config.project.main_scene)
    {
        config.project.main_scene = p;
    }
    config.project.template = crate::catalog::Template::TwoD;
    config.project.engine = crate::scaffold::ENGINE_VERSION.into();
    config.targets.platforms = crate::catalog::TargetPlatform::host().into_iter().collect();
    let mut log = vec![format!("{GODOT_FILE_NAME} -> {} (Godot settings kept; project.godot unchanged)", crate::config::FILE_NAME)];
    let np = crate::scaffold::NewProject {
        name: config.project.name.clone(),
        path: dir.to_path_buf(),
        template: config.project.template,
        backends: config.backends,
        targets: config.targets.platforms.clone(),
        git: false,
        engine_path: engine_path.map(Path::to_path_buf),
    };
    config.save(dir)?;
    for (file, content) in [("Cargo.toml", np.cargo_toml()), ("src/main.rs", np.main_rs())] {
        let path = dir.join(file);
        if !path.exists() {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(io_err(parent))?;
            }
            std::fs::write(&path, content).map_err(io_err(&path))?;
            log.push(format!("created {file} (runs the game with Neko)"));
        }
    }
    // Keep Cargo's build output out of git, next to Godot's own ignores.
    let gi = dir.join(".gitignore");
    if let Ok(g) = std::fs::read_to_string(&gi)
        && !g.lines().any(|l| l.trim() == "/target" || l.trim() == "target/")
    {
        let sep = if g.ends_with('\n') || g.is_empty() { "" } else { "\n" };
        std::fs::write(&gi, format!("{g}{sep}/target\n")).map_err(io_err(&gi))?;
        log.push("added /target to .gitignore".into());
    }
    Ok(log)
}

/// True when the folder is a Neko 0.1 project that needs converting.
pub fn needs_migration(dir: &Path) -> bool {
    dir.join(LEGACY_FILE_NAME).is_file() || !legacy_scenes(dir).is_empty()
}

/// TOML `.scn` files (Godot's binary `.scn` files start with "RSRC"/"RSCC").
fn legacy_scenes(dir: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || name == "target" {
                continue;
            }
            if p.is_dir() && depth < 12 {
                walk(&p, out, depth + 1);
            } else if p.extension().is_some_and(|x| x == "scn") && std::fs::read(&p).is_ok_and(|b| !b.starts_with(b"RSRC") && !b.starts_with(b"RSCC")) {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, &mut out, 0);
    out
}

/// Convert the project in place. Returns one line per change.
pub fn migrate_project(dir: &Path) -> Result<Vec<String>, ProjectError> {
    let mut log = Vec::new();
    for scn in legacy_scenes(dir) {
        let text = std::fs::read_to_string(&scn).map_err(io_err(&scn))?;
        let mut loaded = neko_core::format::from_legacy_toml(&text).map_err(|e| ProjectError::Invalid(format!("{}: {e}", scn.display())))?;
        let tscn = scn.with_extension(neko_core::tscn::EXTENSION);
        std::fs::write(&tscn, neko_core::format::to_string(&mut loaded.scene)).map_err(io_err(&tscn))?;
        std::fs::remove_file(&scn).map_err(io_err(&scn))?;
        log.push(format!("{} -> {}", rel(dir, &scn), rel(dir, &tscn)));
    }
    if dir.join(LEGACY_FILE_NAME).is_file() {
        let mut c = ProjectConfig::load(dir)?;
        if let Some(stem) = c.project.main_scene.strip_suffix(".scn") {
            c.project.main_scene = format!("{stem}.{}", neko_core::tscn::EXTENSION);
        }
        c.save(dir)?;
        log.push(format!("{LEGACY_FILE_NAME} -> {}", crate::config::FILE_NAME));
        // The generated main.rs embeds the project file.
        let main = dir.join("src/main.rs");
        if let Ok(src) = std::fs::read_to_string(&main)
            && src.contains("../neko.toml")
        {
            let new = src.replace("../neko.toml", "../project.neko").replace("`neko.toml`", "`project.neko`");
            std::fs::write(&main, new).map_err(io_err(&main))?;
            log.push("src/main.rs now embeds project.neko".into());
        }
    }
    Ok(log)
}

fn rel(dir: &Path, p: &Path) -> String {
    p.strip_prefix(dir).unwrap_or(p).display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_a_neko_0_1_project() {
        let dir = std::env::temp_dir().join(format!("neko-migrate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("scenes")).unwrap();
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("neko.toml"), "[project]\nname = \"Old\"\ntemplate = \"2d\"\nmain_scene = \"scenes/main.scn\"\nengine = \"0.1.0\"\n\n[backends]\nwindow = \"sdl3\"\nrender = \"wgpu\"\naudio = \"none\"\n\n[targets]\nplatforms = [\"linux\"]\n").unwrap();
        std::fs::write(dir.join("scenes/main.scn"), "format = 1\n\n[[node]]\nname = \"Main\"\ntype = \"Node2D\"\n\n[[node]]\nname = \"Player\"\ntype = \"Sprite2D\"\nparent = \".\"\nprops = { position = [10.0, 20.0], texture = \"res://cat.png\" }\n").unwrap();
        std::fs::write(dir.join("src/main.rs"), "fn main() {\n    neko::run(include_str!(\"../neko.toml\"));\n}\n").unwrap();
        assert!(needs_migration(&dir));
        let log = migrate_project(&dir).unwrap();
        assert_eq!(log.len(), 3, "{log:?}");
        assert!(!needs_migration(&dir));
        let c = ProjectConfig::load(&dir).unwrap();
        assert_eq!(c.project.main_scene, "scenes/main.tscn");
        let tscn = std::fs::read_to_string(dir.join("scenes/main.tscn")).unwrap();
        assert!(tscn.contains("[ext_resource type=\"Texture2D\" path=\"res://cat.png\""), "{tscn}");
        assert!(tscn.contains("position = Vector2(10, 20)"), "{tscn}");
        assert!(std::fs::read_to_string(dir.join("src/main.rs")).unwrap().contains("../project.neko"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn imports_a_godot_project() {
        let dir = std::env::temp_dir().join(format!("neko-godot-import-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let godot = "; Engine configuration file.\n\nconfig_version=5\n\n[application]\n\nconfig/name=\"Dodge\"\nrun/main_scene=\"uid://cd3k1x0c5oqfb\"\nconfig/features=PackedStringArray(\"4.7\", \"Forward Plus\")\n\n[display]\n\nwindow/size/viewport_width=480\n";
        std::fs::write(dir.join("project.godot"), godot).unwrap();
        std::fs::write(dir.join("main.tscn"), "[gd_scene format=3 uid=\"uid://cd3k1x0c5oqfb\"]\n\n[node name=\"Main\" type=\"Node2D\"]\n").unwrap();
        std::fs::write(dir.join(".gitignore"), "# Godot 4+ specific ignores\n.godot/\n").unwrap();
        let log = prepare_project(&dir, None).unwrap();
        assert!(log.len() >= 3, "{log:?}");
        let c = ProjectConfig::load(&dir).unwrap();
        assert_eq!(c.project.name, "Dodge");
        assert_eq!(c.project.main_scene, "main.tscn");
        let neko = std::fs::read_to_string(dir.join("project.neko")).unwrap();
        assert!(neko.contains("window/size/viewport_width=480"), "{neko}");
        assert_eq!(std::fs::read_to_string(dir.join("project.godot")).unwrap(), godot);
        assert!(std::fs::read_to_string(dir.join("src/main.rs")).unwrap().contains("project.neko"));
        assert!(std::fs::read_to_string(dir.join(".gitignore")).unwrap().contains("/target"));
        assert!(prepare_project(&dir, None).unwrap().is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
