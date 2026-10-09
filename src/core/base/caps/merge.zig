// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! Compile-time helper that folds several capability structs into one flat
//! vtable type.
//!
//! Each capability module (`core.zig`, `window.zig`, …) declares its function
//! pointers with a **default no-op implementation**. `merge` concatenates the
//! fields in order and keeps those defaults, so a backend only has to name the
//! capabilities it actually supports; everything else falls back to a no-op.
//!
//! The result is a single plain struct, so existing code that indexes the
//! vtable directly (`vtable.draw_rect`, `engine.Backend.VTable{ .init = … }`)
//! keeps working unchanged.

const std: type = @import("std");

/// Folds `parts` (structs of function-pointer fields, with defaults) into one
/// struct type. Field order follows `parts`; duplicate names are a compile
/// error.
pub fn merge(comptime parts: []const type) type {
    comptime var count: usize = 0;
    inline for (parts) |Part| {
        count += @typeInfo(Part).@"struct".fields.len;
    }

    var names: [count][]const u8 = undefined;
    var types: [count]type = undefined;
    var attrs: [count]std.builtin.Type.StructField.Attributes = undefined;

    comptime var index: usize = 0;
    inline for (parts) |Part| {
        inline for (@typeInfo(Part).@"struct".fields) |field| {
            const typed: std.builtin.Type.StructField = field;
            names[index] = typed.name;
            types[index] = typed.type;
            attrs[index] = .{
                .default_value_ptr = typed.default_value_ptr,
                .@"comptime" = typed.is_comptime,
                .@"align" = typed.alignment,
            };
            index += 1;
        }
    }

    return @Struct(.auto, null, &names, &types, &attrs);
}
