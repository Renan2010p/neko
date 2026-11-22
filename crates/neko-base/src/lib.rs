//! # neko-base
//!
//! What the engine core needs from "below", without an operating system
//! (Godot keeps the same pieces in `core/math`, `core/templates` and
//! `core/os`). Everything here works with `#![no_std]` + `alloc` on targets
//! with no atomics and no FPU, such as the PlayStation (`mipsel-sony-psx`).
//!
//! - [`math::FloatExt`]: `sin`, `sqrt`, `powf`... for `f32`/`f64` via `libm`
//!   (core Rust has no float math without an OS).
//! - [`collections`]: `HashMap`/`HashSet` with a deterministic hasher (no
//!   random seed, so no OS entropy and identical iteration on every target).
//! - [`prelude`]: `String`, `Vec`, `Box`, `format!`... from `alloc`.
//!
//! There is no global mutable state and no `unsafe`: the engine stays safe to
//! run on several threads, and nothing here needs atomics.

#![no_std]

extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

pub mod collections;
pub mod math;
pub mod prelude;
