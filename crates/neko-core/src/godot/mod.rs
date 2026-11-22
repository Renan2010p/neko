//! Godot's text formats, so Godot projects open in Neko and back:
//!
//! - [`value`]: the value syntax shared by every text format
//!   (`Vector2(1, 2)`, `&"name"`, `ExtResource("1_abc")`...), after
//!   core/variant/variant_parser.cpp;
//! - [`text`]: `.tscn` / `.tres` documents as tagged sections, after
//!   scene/resources/resource_format_text.cpp;
//! - [`config`]: `ConfigFile`, the syntax of `project.godot` and Neko's
//!   `project.neko`, after core/io/config_file.cpp;
//! - [`uid`]: `uid://` identifiers, after core/io/resource_uid.cpp.
//!
//! Everything round-trips: values and sections Neko does not understand are
//! kept verbatim so saving never loses data from a Godot project.

#[allow(unused_imports)]
use neko_base::prelude::*;

pub mod config;
pub mod text;
pub mod uid;
pub mod value;

pub use value::Value;
