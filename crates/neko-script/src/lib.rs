//! # NekoScript
//!
//! Neko's scripting language. It is GDScript: the same syntax and semantics
//! as Godot 4's, so `.gd` files from Godot projects run in Neko unchanged.
//! The implementation follows Godot's `modules/gdscript`:
//!
//! - [`lexer`] is `GDScriptTokenizer`;
//! - [`ast`] and [`parser`] are `GDScriptParser`;
//! - [`interp`], [`value`] and [`builtins`] are the VM and `@GlobalScope`.

//!
//! It is freestanding (`no_std` + `alloc`): the same interpreter runs in the
//! editor and in games on any backend. It keeps no global mutable state, so
//! several interpreters can run on different threads.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod ast;
pub mod builtins;
pub mod interp;
pub mod lexer;
pub mod parser;
pub mod value;

pub use interp::{Callable, Host, Interp, NullHost, ObjKind, Object, ScriptClass, ScriptError};
pub use value::{Dict, Value};

pub use parser::{ParseError, parse};

#[cfg(test)]
mod tests;
