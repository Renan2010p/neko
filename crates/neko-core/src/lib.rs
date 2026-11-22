//! # neko-core
//!
//! The engine's data model, with no rendering or platform code:
//!
//! - [`variant`]: dynamically typed property values.
//! - [`classdb`]: every node class, its parent, properties and docs.
//! - [`scene`]: the node tree (an arena with stable [`NodeId`]s).
//! - [`format`] and [`tscn`]: Godot `.tscn` scenes (and Neko 0.1's TOML scenes with `std`).
//!
//! It is freestanding (`no_std` + `alloc`, see `neko-base`): the same scene
//! code runs in the editor and in games on any backend, down to the PS1.
//! With the `std` feature (default) it also reads legacy TOML scenes and
//! takes new ids from the OS.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod classdb;
pub mod format;
pub mod godot;
pub mod tscn;
pub mod scene;
pub mod variant;

pub use classdb::{Category, ClassDb, ClassInfo, PropDef};
pub use scene::{Node, NodeId, Scene, SceneError};
pub use variant::{PropType, Variant};
