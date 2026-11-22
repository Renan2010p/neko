//! Create a project from the command line, like the editor's "Create" dialog.
//!
//! `cargo run -p neko-project --example new_project -- <folder> "<name>" [engine-path]`

use std::path::PathBuf;

use neko_project::{BackendSet, NewProject, TargetPlatform, Template, create_project};

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(path), Some(name)) = (args.next(), args.next()) else {
        eprintln!("usage: new_project <folder> <name> [engine-path]");
        std::process::exit(2);
    };
    let p = NewProject {
        name,
        path: PathBuf::from(path),
        template: Template::TwoD,
        backends: BackendSet { audio: neko_project::AudioBackend::None, ..BackendSet::default() },
        targets: vec![TargetPlatform::Linux],
        git: false,
        engine_path: args.next().map(PathBuf::from),
    };
    match create_project(&p) {
        Ok(dir) => println!("created {}", dir.display()),
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}
