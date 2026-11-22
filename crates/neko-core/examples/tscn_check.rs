//! Check Godot scenes against Neko's .tscn reader and writer, read-only:
//! each file is loaded, written back in memory and compared byte for byte.
//!
//! `cargo run -p neko-core --example tscn_check -- <scene.tscn or folder>...`

use std::path::{Path, PathBuf};

fn collect(p: &Path, out: &mut Vec<PathBuf>) {
    if p.is_dir() {
        let Ok(rd) = std::fs::read_dir(p) else { return };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || name == "addons" || name == "target" {
                continue;
            }
            collect(&e.path(), out);
        }
    } else if p.extension().is_some_and(|e| e == "tscn") {
        out.push(p.to_path_buf());
    }
}

/// Drop what Godot 4.7 itself rewrites when saving a file from an older
/// version: `load_steps`, the scene `uid` and node `unique_id`s.
fn normalize(text: &str) -> String {
    text.lines()
        .map(|l| {
            let mut l = l.to_string();
            if l.starts_with("[gd_scene") {
                return "[gd_scene]".to_string();
            }
            if l.starts_with("[node ")
                && let Some(i) = l.find(" unique_id=")
            {
                let end = l[i + 1..].find(' ').map_or(l.len() - 1, |j| i + 1 + j);
                l.replace_range(i..end, "");
            }
            l
        })
        .collect::<Vec<_>>()
        .join("\n")
        // Hand-edited files may have extra blank lines that Godot drops too.
        .split("\n\n\n")
        .collect::<Vec<_>>()
        .join("\n\n")
        .trim_end()
        .to_string()
}

fn main() {
    // NEKO_WRITE_SAMPLE=<file>: write a scene built in Neko, to open in Godot.
    if let Some(out) = std::env::var_os("NEKO_WRITE_SAMPLE") {
        use neko_core::{Scene, Variant};
        let mut s = Scene::new("Node2D", "Main").unwrap();
        let root = s.root();
        let p = s.add_child(root, "CharacterBody2D", Some("Player")).unwrap();
        s.get_mut(p).unwrap().set("position", Variant::Vec2([300.0, 200.0])).unwrap();
        s.get_mut(p).unwrap().set("rotation_degrees", Variant::Float(30.0)).unwrap();
        let spr = s.add_child(p, "Sprite2D", None).unwrap();
        s.get_mut(spr).unwrap().set("texture", Variant::str("res://icon.svg")).unwrap();
        let col = s.add_child(p, "CollisionShape2D", None).unwrap();
        s.get_mut(col).unwrap().set("size", Variant::Vec2([40.0, 60.0])).unwrap();
        let ui = s.add_child(root, "Label", Some("Score")).unwrap();
        s.get_mut(ui).unwrap().set("text", Variant::str("Score: 0")).unwrap();
        s.get_mut(ui).unwrap().set("position", Variant::Vec2([10.0, 10.0])).unwrap();
        s.get_mut(ui).unwrap().set("horizontal_alignment", Variant::str("center")).unwrap();
        let w = s.add_child(root, "Node3D", Some("World3D")).unwrap();
        s.get_mut(w).unwrap().set("rotation_degrees", Variant::Vec3([0.0, 90.0, 0.0])).unwrap();
        s.get_mut(w).unwrap().set("position", Variant::Vec3([1.0, 2.0, 3.0])).unwrap();
        std::fs::write(out, neko_core::format::to_string(&mut s)).unwrap();
        return;
    }
    // NEKO_CHECK_CONFIG=<project.godot>: round-trip a ConfigFile, read-only.
    if let Some(p) = std::env::var_os("NEKO_CHECK_CONFIG") {
        let text = std::fs::read_to_string(&p).unwrap();
        let c = neko_core::godot::config::ConfigFile::parse(&text).unwrap_or_else(|e| panic!("{e}"));
        // Compare everything after the comment header.
        let body = |t: &str| t.lines().skip_while(|l| l.starts_with(';') || l.is_empty()).collect::<Vec<_>>().join("\n");
        let (text, out) = (body(&text), body(&c.write("")));
        println!("{} sections, identical after round trip: {}", c.sections.len(), out == text);
        if out != text {
            for (i, (a, b)) in text.lines().zip(out.lines()).enumerate() {
                if a != b {
                    println!("line {}:\n  godot: {a}\n  neko:  {b}", i + 1);
                    break;
                }
            }
        }
        return;
    }
    let mut files = Vec::new();
    for a in std::env::args().skip(1) {
        collect(Path::new(&a), &mut files);
    }
    let (mut same, mut changed, mut failed) = (0, 0, 0);
    let mut unknown: std::collections::BTreeMap<String, usize> = Default::default();
    for f in &files {
        let text = std::fs::read_to_string(f).unwrap_or_default();
        match neko_core::format::from_str(&text) {
            Ok(mut l) => {
                for w in &l.warnings {
                    if let Some(c) = w.split("Neko has no ").nth(1).and_then(|r| r.split(' ').next()) {
                        *unknown.entry(c.trim_end_matches(';').to_string()).or_default() += 1;
                    }
                }
                let out = neko_core::format::to_string(&mut l.scene);
                if let Some(dir) = std::env::var_os("TSCN_DUMP") {
                    let _ = std::fs::write(Path::new(&dir).join(f.to_string_lossy().replace('/', "__")), &out);
                }
                if normalize(&out) == normalize(&text) {
                    same += 1;
                } else {
                    changed += 1;
                    if changed <= 3 {
                        println!("CHANGED {}", f.display());
                        let (text, out) = (normalize(&text), normalize(&out));
                        for (i, (a, b)) in text.lines().zip(out.lines()).enumerate() {
                            if a != b {
                                println!("  line {}:\n    godot: {a}\n    neko:  {b}", i + 1);
                                break;
                            }
                        }
                        if text.lines().count() != out.lines().count() {
                            println!("  lines: godot {} neko {}", text.lines().count(), out.lines().count());
                        }
                    }
                }
            }
            Err(e) => {
                failed += 1;
                println!("FAILED {}: {e}", f.display());
            }
        }
    }
    println!("\n{} scenes: {same} unchanged after load+save (ignoring the 4.7 header/unique_id update), {changed} changed, {failed} failed", files.len());
    if !unknown.is_empty() {
        println!("Godot classes Neko does not have yet (kept as-is): {unknown:?}");
    }
}
