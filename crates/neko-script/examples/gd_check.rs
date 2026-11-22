//! Read-only check of `.gd` files against NekoScript's tokenizer and parser.
//!
//! `cargo run -p neko-script --example gd_check -- <folder or .gd>...`

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
    } else if p.extension().is_some_and(|e| e == "gd") {
        out.push(p.to_path_buf());
    }
}

fn main() {
    let mut files = Vec::new();
    for a in std::env::args().skip(1) {
        collect(Path::new(&a), &mut files);
    }
    let (mut ok, mut failed) = (0, 0);
    for f in &files {
        let src = std::fs::read_to_string(f).unwrap_or_default();
        match check(&src) {
            Ok(()) => ok += 1,
            Err(e) => {
                failed += 1;
                if failed <= 15 {
                    println!("{}: {e}", f.display());
                }
            }
        }
    }
    println!("\n{} scripts: {ok} ok, {failed} failed", files.len());
}

fn check(src: &str) -> Result<(), String> {
    neko_script::parse(src).map_err(|e| e.to_string())?;
    Ok(())
}
