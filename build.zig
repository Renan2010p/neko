// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! Neko — a small general-purpose 2D game engine.
//!
//! ## What this package exposes
//!
//!   - `neko`          the engine itself (public `neko.*` namespaces)
//!   - `neko_backend`  the selected backend, a stable module name so game code
//!                     never changes when you switch backends
//!
//! ## Backends are plugins
//!
//! A backend lives in `src/platform/<name>/` and is registered in
//! `build/backends.zig`. Adding one means writing `build/backends/<name>.zig`
//! (a `plugin: Backend`) and listing it there — `build.zig` never changes. The
//! selected backend is chosen with `-Dbackend=<name>`:
//!
//!     zig build -Dbackend=ps2
//!
//! ## Steps
//!
//!   - `zig build`              build the engine module (no artifacts)
//!   - `zig build test`         run the unit tests
//!   - `zig build python`       build the `_neko` CPython extension + pygame
//!   - `zig build python-demo`  run the bundled pygame demo
//!   - `zig build docs`         API reference into `zig-out/docs/api`
//!   - `zig build backends`     list the available backends

const std: type = @import("std");
const builtin: type = std.builtin;
const Builder: type = std.Build;

const backend: type = @import("build/backend.zig");
const backends: type = @import("build/backends.zig");

pub fn build(b: *Builder) void {
    const target: Builder.ResolvedTarget = b.standardTargetOptions(.{});
    // Games are performance-sensitive, so an unqualified `zig build` uses
    // ReleaseFast. Override with `-Doptimize=Debug` (or Safe/Small/ReleaseFast).
    const optimize: builtin.OptimizeMode = b.option(
        builtin.OptimizeMode,
        "optimize",
        "Prioritize performance, safety, or binary size",
    ) orelse .ReleaseFast;

    // The backend is a name looked up in the registry (`build/backends.zig`),
    // so adding one never requires editing this file.
    const backend_name: []const u8 = b.option(
        []const u8,
        "backend",
        b.fmt("Backend to build ({s})", .{backends.names}),
    ) orelse backends.all[0].name;
    const plugin: backend.Backend = backends.find(backend_name) orelse {
        std.debug.print("neko: unknown backend '{s}'; available: {s}\n", .{ backend_name, backends.names });
        @panic("neko: unknown backend");
    };

    // Optional system-library directories for the SDL2 backend (Windows has no
    // pkg-config): `-Dsdl2-include=... -Dsdl2-lib=...`.
    const sdl2_include: ?[]const u8 = b.option(
        []const u8,
        "sdl2-include",
        "Directory containing SDL2.h / SDL2/ (SDL2 backend only)",
    );
    const sdl2_lib: ?[]const u8 = b.option(
        []const u8,
        "sdl2-lib",
        "Directory containing the SDL2 import libraries (SDL2 backend only)",
    );

    // A freestanding target (the PS2) has no window, no test runner and no
    // browser, so the developer steps are hosted-only.
    const hosted: bool = target.result.os.tag != .freestanding;

    // ── Engine module ────────────────────────────────────────────────────
    const neko: *Builder.Module = b.addModule("neko", .{
        .root_source_file = b.path("src/neko.zig"),
        .target = target,
        .optimize = optimize,
        .link_libc = hosted,
    });

    // ── pygame compatibility module ──────────────────────────────────────
    // A pygame-shaped software layer built only on neko's public API.
    const pygame: *Builder.Module = b.addModule("neko_pygame", .{
        .root_source_file = b.path("src/compat/pygame.zig"),
        .target = target,
        .optimize = optimize,
        .link_libc = hosted,
        .imports = &.{.{ .name = "neko", .module = neko }},
    });

    // ── Backend module ───────────────────────────────────────────────────
    // Published under the stable name `neko_backend`.
    const ctx: backend.Context = .{
        .b = b,
        .neko = neko,
        .target = target,
        .optimize = optimize,
        .sdl2_include = sdl2_include,
        .sdl2_lib = sdl2_lib,
    };
    const backend_module: *Builder.Module = plugin.build(ctx);

    // The engine's `screen` module instantiates the backend; cyclical module
    // imports are allowed in Zig.
    neko.addImport("neko_backend", backend_module);

    // ── Developer steps ──────────────────────────────────────────────────
    if (hosted) {
        add_tests(b, target, optimize, plugin);
        add_docs(b, neko);
        add_python(b, neko, pygame, target, optimize, sdl2_include, sdl2_lib, plugin);
    }

    const list: *Builder.Step.Run = b.addSystemCommand(&.{ "echo", backends.names });
    const list_step: *Builder.Step = b.step("backends", "List the available backends");
    list_step.dependOn(&list.step);
}

// ── Tests ────────────────────────────────────────────────────────────────────

/// Adds `zig build test`, running the unit tests in the core modules.
fn add_tests(
    b: *Builder,
    target: Builder.ResolvedTarget,
    optimize: builtin.OptimizeMode,
    plugin: backend.Backend,
) void {
    const module: *Builder.Module = b.createModule(.{
        .root_source_file = b.path("src/tests.zig"),
        .target = target,
        .optimize = optimize,
        .link_libc = true,
    });
    plugin.link(module);

    const tests: *Builder.Step.Compile = b.addTest(.{ .root_module = module });
    const run_tests: *Builder.Step.Run = b.addRunArtifact(tests);

    const step: *Builder.Step = b.step("test", "Run the unit tests");
    step.dependOn(&run_tests.step);
}

// ── Python bindings ──────────────────────────────────────────────────────────

/// Adds `zig build python`: the CPython extension `_neko` (in Zig) plus the
/// pure-Python `pygame` package under `zig-out/python/`. Put that directory on
/// `PYTHONPATH` to run pygame games on Neko.
fn add_python(
    b: *Builder,
    neko: *Builder.Module,
    pygame: *Builder.Module,
    target: Builder.ResolvedTarget,
    optimize: builtin.OptimizeMode,
    sdl2_include: ?[]const u8,
    sdl2_lib: ?[]const u8,
    plugin: backend.Backend,
) void {
    const python_include: []const u8 = b.option(
        []const u8,
        "python-include",
        "Directory containing Python.h (default: /usr/include/python3.14)",
    ) orelse "/usr/include/python3.14";

    const ext_module: *Builder.Module = b.createModule(.{
        .root_source_file = b.path("bindings/python/neko_ext.zig"),
        .target = target,
        .optimize = optimize,
        .link_libc = true,
        .imports = &.{
            .{ .name = "neko", .module = neko },
            .{ .name = "neko_pygame", .module = pygame },
        },
    });
    ext_module.addIncludePath(.{ .cwd_relative = python_include });
    if (sdl2_include) |inc| ext_module.addIncludePath(.{ .cwd_relative = inc });
    if (sdl2_lib) |lib| ext_module.addLibraryPath(.{ .cwd_relative = lib });

    const ext: *Builder.Step.Compile = b.addLibrary(.{
        .linkage = .dynamic,
        .name = "_neko",
        .root_module = ext_module,
    });
    plugin.link(ext.root_module);
    // A Python extension leaves the CPython symbols unresolved; the running
    // interpreter provides them at import time.
    ext.linker_allow_shlib_undefined = true;

    const step: *Builder.Step = b.step("python", "Build the pygame-compatible Python extension");
    const install_lib: *Builder.Step.InstallFile = b.addInstallFileWithDir(
        ext.getEmittedBin(),
        .{ .custom = "python/pygame" },
        "_neko.so",
    );
    const install_pkg: *Builder.Step.InstallDir = b.addInstallDirectory(.{
        .source_dir = b.path("bindings/python/pygame"),
        .install_dir = .{ .custom = "python" },
        .install_subdir = "pygame",
    });
    const install_demo: *Builder.Step.InstallFile = b.addInstallFileWithDir(
        b.path("bindings/python/demo.py"),
        .{ .custom = "python" },
        "demo.py",
    );
    step.dependOn(&install_lib.step);
    step.dependOn(&install_pkg.step);
    step.dependOn(&install_demo.step);

    // `zig build python-demo` runs it against the freshly built engine.
    const demo: *Builder.Step.Run = b.addSystemCommand(&.{ "python3", "demo.py" });
    demo.setEnvironmentVariable("PYTHONPATH", b.getInstallPath(.prefix, "python"));
    demo.setCwd(.{ .cwd_relative = b.getInstallPath(.prefix, "python") });
    demo.step.dependOn(&install_lib.step);
    demo.step.dependOn(&install_pkg.step);
    demo.step.dependOn(&install_demo.step);
    const demo_step: *Builder.Step = b.step("python-demo", "Run the pygame demo on Neko");
    demo_step.dependOn(&demo.step);
}

// ── API docs ─────────────────────────────────────────────────────────────────

/// Adds `zig build docs`, emitting Zig autodoc to `zig-out/docs/api`.
fn add_docs(b: *Builder, neko: *Builder.Module) void {
    const object: *Builder.Step.Compile = b.addObject(.{
        .name = "neko",
        .root_module = neko,
    });

    const install: *Builder.Step.InstallDir = b.addInstallDirectory(.{
        .source_dir = object.getEmittedDocs(),
        .install_dir = .prefix,
        .install_subdir = "docs/api",
    });

    const step: *Builder.Step = b.step("docs", "Build the API reference into zig-out/docs/api");
    step.dependOn(&install.step);
}
