//! Scene files.
//!
//! Neko reads and writes Godot's `.tscn` (see [`crate::godot`] and the
//! `tscn` module). This file also keeps the loader for Neko 0.1's TOML
//! `.scn` scenes, used to migrate old projects:
//!
//! ```toml
//! format = 1
//!
//! [[node]]
//! name = "Main"
//! type = "Node2D"
//!
//! [[node]]
//! name = "Player"
//! type = "CharacterBody2D"
//! parent = "."
//! props = { position = [64.0, 32.0] }
//! ```
//!
//! Nodes are listed parents-first; `parent` is a path relative to the root.
//! Only properties that differ from the class default are written.

#[allow(unused_imports)]
use neko_base::prelude::*;

#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};

#[cfg(feature = "std")]
use crate::classdb::ClassDb;
use crate::scene::Scene;
#[cfg(feature = "std")]
use crate::variant::Variant;

/// Version of the legacy TOML format.
pub const FORMAT_VERSION: u32 = 1;

#[cfg(feature = "std")]
pub use crate::tscn::to_string;
pub use crate::tscn::{EXTENSION, from_str, to_string_with};

#[cfg(feature = "std")]
#[derive(Serialize, Deserialize)]
struct SceneFile {
    format: u32,
    #[serde(default)]
    node: Vec<NodeEntry>,
}

#[cfg(feature = "std")]
#[derive(Serialize, Deserialize)]
struct NodeEntry {
    name: String,
    #[serde(rename = "type")]
    class: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    parent: Option<String>,
    #[serde(default, skip_serializing_if = "toml::Table::is_empty")]
    props: toml::Table,
}

#[derive(Debug, thiserror::Error)]
pub enum FormatError {
    #[error("not a valid scene file: {0}")]
    Syntax(String),
    #[error("scene format {0} is newer than this editor supports ({FORMAT_VERSION})")]
    TooNew(u32),
    #[error("the scene has no nodes")]
    Empty,
    #[error("root node: {0}")]
    Root(String),
}

/// A loaded scene plus anything that was skipped while loading.
pub struct Loaded {
    pub scene: Scene,
    pub warnings: Vec<String>,
}

/// Load a scene from the pre-`.tscn` TOML format (Neko 0.1 `.scn` files).
/// Without `std` this format is not available (export converts scenes).
#[cfg(not(feature = "std"))]
pub fn from_legacy_toml(_text: &str) -> Result<Loaded, FormatError> {
    Err(FormatError::Syntax("legacy TOML scenes need the std feature".into()))
}

/// Load a scene from the pre-`.tscn` TOML format (Neko 0.1 `.scn` files).
#[cfg(feature = "std")]
pub fn from_legacy_toml(text: &str) -> Result<Loaded, FormatError> {
    let file: SceneFile = toml::from_str(text).map_err(|e| FormatError::Syntax(e.to_string()))?;
    if file.format > FORMAT_VERSION {
        return Err(FormatError::TooNew(file.format));
    }
    let mut nodes = file.node.into_iter();
    let root = nodes.next().ok_or(FormatError::Empty)?;
    let mut scene = Scene::new(&root.class, &root.name).map_err(|e| FormatError::Root(e.to_string()))?;
    let mut warnings = Vec::new();
    let root_id = scene.root();
    apply_props(&mut scene, root_id, &root.props, &mut warnings);
    for entry in nodes {
        let Some(parent) = entry.parent.as_deref().and_then(|p| scene.find(p)) else {
            warnings.push(format!("'{}' skipped: parent '{}' not found", entry.name, entry.parent.unwrap_or_default()));
            continue;
        };
        let id = match scene.add_child(parent, &entry.class, Some(&entry.name)) {
            Ok(id) => id,
            Err(e) => {
                warnings.push(format!("'{}' loaded as Node: {e}", entry.name));
                scene.add_child(parent, "Node", Some(&entry.name)).expect("Node is always valid")
            }
        };
        apply_props(&mut scene, id, &entry.props, &mut warnings);
    }
    Ok(Loaded { scene, warnings })
}

#[cfg(feature = "std")]
fn apply_props(scene: &mut Scene, id: crate::NodeId, props: &toml::Table, warnings: &mut Vec<String>) {
    let node = scene.get_mut(id).expect("just created");
    for (k, v) in props {
        let Some(def) = ClassDb::get().property(node.class, k) else {
            warnings.push(format!("{}: unknown property '{k}' dropped", node.name));
            continue;
        };
        match Variant::from_toml(def.ty, v) {
            Some(val) => {
                let _ = node.set(k, val);
            }
            None => warnings.push(format!("{}: property '{k}' has the wrong type, using the default", node.name)),
        }
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let mut s = Scene::new("Node2D", "Main").unwrap();
        let p = s.add_child(s.root(), "CharacterBody2D", Some("Player")).unwrap();
        let spr = s.add_child(p, "Sprite2D", None).unwrap();
        s.get_mut(p).unwrap().set("position", Variant::Vec2([64.0, 32.5])).unwrap();
        s.get_mut(spr).unwrap().set("texture", Variant::str("res://cat.png")).unwrap();
        s.get_mut(spr).unwrap().set("flip_h", Variant::Bool(true)).unwrap();
        let text = to_string(&mut s);
        let mut loaded = from_str(&text).unwrap();
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        assert_eq!(to_string(&mut loaded.scene), text);
        let spr2 = loaded.scene.find("Player/Sprite2D").unwrap();
        assert_eq!(loaded.scene.get(spr2).unwrap().get("flip_h"), Some(Variant::Bool(true)));
    }

    #[test]
    fn bad_entries_become_warnings() {
        let text = r#"
format = 1
[[node]]
name = "Main"
type = "Node2D"
[[node]]
name = "Ghost"
type = "NoSuchClass"
parent = "."
props = { position = [1, 2], color = "red" }
[[node]]
name = "Orphan"
type = "Node"
parent = "Missing"
"#;
        let l = from_str(text).unwrap();
        assert_eq!(l.scene.len(), 2);
        assert_eq!(l.warnings.len(), 4, "{:?}", l.warnings);
    }
}
