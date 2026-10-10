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
//! A backend lives in `src/backend/<Name>/` and is registered in
//! `src/backend/registry.zig`. Adding one means writing a `plugin: Backend`
//! (a `build.zig` next to the renderer) and listing it there — `build.zig` never
//! changes. The
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

const backend: type = @import("src/backend/plugin.zig");
const backends: type = @import("src/backend/registry.zig");

pub fn build(b: *Builder) void {
    const target: Builder.ResolvedTarget = b.standardTargetOptions(.{});
    // Games are performance-sensitive, so an unqualified `zig build` uses
    // ReleaseFast. Override with `-Doptimize=Debug` (or Safe/Small/ReleaseFast).
    const optimize: builtin.OptimizeMode = b.option(
        builtin.OptimizeMode,
        "optimize",
        "Prioritize performance, safety, or binary size",
    ) orelse .ReleaseFast;

    // The backend is a name looked up in the registry (`src/backend/registry.zig`),
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
        .root_source_file = b.path("src/compat/pygame/pygame.zig"),
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
        const freestanding_check: *Builder.Step = add_freestanding_check(b);
        add_tests(b, neko, freestanding_check);
        add_docs(b, neko);
        add_python(b, neko, pygame, target, optimize, sdl2_include, sdl2_lib, plugin);
        _ = add_target_check(b, optimize);
    }

    const list: *Builder.Step.Run = b.addSystemCommand(&.{ "echo", backends.names });
    const list_step: *Builder.Step = b.step("backends", "List the available backends");
    list_step.dependOn(&list.step);
}

// ── Tests ────────────────────────────────────────────────────────────────────

/// Adds `zig build test`, running the unit tests in the core modules.
///
/// The test root is the `neko` module itself (`src/neko.zig`, the only file at
/// `src/`), so the whole engine tree is part of the test compilation; the unit
/// tests live next to the code they test and `src/test/root.zig` gathers any
/// extra test-only files.
fn add_tests(b: *Builder, neko: *Builder.Module, extra: *Builder.Step) void {
    const tests: *Builder.Step.Compile = b.addTest(.{ .root_module = neko });
    const run_tests: *Builder.Step.Run = b.addRunArtifact(tests);

    const step: *Builder.Step = b.step("test", "Run the unit tests");
    step.dependOn(&run_tests.step);
    step.dependOn(extra);
}

// ── Portability guard: cross-compile the core for many targets ───────────────

/// Adds `zig build check-targets`: compiles the `neko` core plus the `headless`
/// backend for a matrix of targets (hosted, embedded, big-endian, wasm). It
/// never links or runs, so no SDK is needed; it just proves the core has no
/// target-specific assumptions. A new backend or core change that breaks a
/// target fails here.
fn add_target_check(b: *Builder, optimize: builtin.OptimizeMode) *Builder.Step {
    const targets: []const []const u8 = &.{
        "x86_64-linux-gnu",
        "aarch64-linux-gnu",
        "riscv64-linux-gnu",
        "x86_64-windows-gnu",
        "aarch64-macos-none",
        "x86_64-macos-none",
        "wasm32-wasi",
        "wasm32-freestanding",
        "arm-freestanding-eabi",
        "thumb-freestanding-eabi",
        "mips-freestanding",
        "mipsel-freestanding",
        "powerpc-freestanding",
    };

    const headless: backend.Backend = backends.find("headless") orelse @panic("neko: headless backend missing");
    const step: *Builder.Step = b.step("check-targets", "Cross-compile the core + headless for many targets");

    for (targets) |triple| {
        const query: std.Target.Query = std.Target.Query.parse(.{ .arch_os_abi = triple }) catch |err| {
            std.debug.print("neko: check-targets: bad target '{s}': {s}\n", .{ triple, @errorName(err) });
            continue;
        };
        const resolved: Builder.ResolvedTarget = b.resolveTargetQuery(query);

        const neko_mod: *Builder.Module = b.createModule(.{
            .root_source_file = b.path("src/neko.zig"),
            .target = resolved,
            .optimize = optimize,
        });
        const ctx: backend.Context = .{
            .b = b,
            .neko = neko_mod,
            .target = resolved,
            .optimize = optimize,
            // The same backend is built for every target, so its modules must
            // be private: publishing a name twice is an error in Zig 0.17.
            .publish = false,
        };
        const backend_mod: *Builder.Module = headless.build(ctx);
        neko_mod.addImport("neko_backend", backend_mod);

        const obj: *Builder.Step.Compile = b.addObject(.{
            .name = b.fmt("neko-{s}", .{triple}),
            .root_module = neko_mod,
        });
        step.dependOn(&obj.step);
    }

    return step;
}

// ── Freestanding guard ───────────────────────────────────────────────────────

/// Adds `zig build check-freestanding`: fail if `src/core/**` references an OS
/// API. Runs as part of `zig build test`.
fn add_freestanding_check(b: *Builder) *Builder.Step {
    const tool: *Builder.Step.Compile = b.addExecutable(.{
        .name = "check-freestanding",
        .root_module = b.createModule(.{
            .root_source_file = b.path("tools/check_freestanding.zig"),
            .target = b.graph.host,
            .optimize = .ReleaseSafe,
        }),
    });
    const run: *Builder.Step.Run = b.addRunArtifact(tool);
    run.addArg("src/core");
    run.setCwd(b.path("."));

    const step: *Builder.Step = b.step("check-freestanding", "Fail if src/core touches an OS API");
    step.dependOn(&run.step);
    return &run.step;
}

// ── Python bindings ──────────────────────────────────────────────────────────

/// The Python C API as a Zig module. `@cImport` was removed in Zig 0.17, so the
/// header is run through `translate-c` and published under the `c` name.
fn python_c_module(
    b: *Builder,
    target: Builder.ResolvedTarget,
    optimize: builtin.OptimizeMode,
    python_include: []const u8,
) *Builder.Module {
    const translate: *Builder.Step.TranslateC = b.addTranslateC(.{
        .root_source_file = b.path("bindings/python/c.h"),
        .target = target,
        .optimize = optimize,
        .link_libc = true,
    });
    translate.addIncludePath(.{ .cwd_relative = python_include });
    translate.defineCMacro("PY_SSIZE_T_CLEAN", null);
    return translate.createModule();
}

/// Asks the `python3` on `PATH` for its include directory, so the bindings are
/// built against the interpreter that will import them (`actions/setup-python`
/// and distro Pythons disagree on the version-stamped path). Falls back to the
/// version-stamped system path when `python3` is unavailable.
fn detect_python_include(b: *Builder) []const u8 {
    const result: Builder.RunResult = b.runFallible(
        &.{ "python3", "-c", "import sysconfig; print(sysconfig.get_path('include'))" },
        .{ .stderr_behavior = .ignore },
    );
    switch (result) {
        .success => |stdout| {
            const include: []const u8 = std.mem.trim(u8, stdout, " \t\r\n");
            if (include.len != 0) return include;
        },
        else => {},
    }
    return "/usr/include/python3.14";
}

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
        "Directory containing Python.h (default: detected from python3)",
    ) orelse detect_python_include(b);

    const ext_module: *Builder.Module = b.createModule(.{
        .root_source_file = b.path("bindings/python/neko_ext.zig"),
        .target = target,
        .optimize = optimize,
        .link_libc = true,
        .imports = &.{
            .{ .name = "c", .module = python_c_module(b, target, optimize, python_include) },
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
    // The Ursina 3D layer: its own copy of the extension plus the pure-Python
    // `ursina` package and a demo.
    const install_ursina_lib: *Builder.Step.InstallFile = b.addInstallFileWithDir(
        ext.getEmittedBin(),
        .{ .custom = "python/ursina" },
        "_neko.so",
    );
    const install_ursina_pkg: *Builder.Step.InstallDir = b.addInstallDirectory(.{
        .source_dir = b.path("bindings/python/ursina"),
        .install_dir = .{ .custom = "python" },
        .install_subdir = "ursina",
    });
    const install_ursina_demo: *Builder.Step.InstallFile = b.addInstallFileWithDir(
        b.path("bindings/python/ursina_demo.py"),
        .{ .custom = "python" },
        "ursina_demo.py",
    );
    step.dependOn(&install_lib.step);
    step.dependOn(&install_pkg.step);
    step.dependOn(&install_demo.step);
    step.dependOn(&install_ursina_lib.step);
    step.dependOn(&install_ursina_pkg.step);
    step.dependOn(&install_ursina_demo.step);

    // `zig build python-demo` runs it against the freshly built engine.
    const demo: *Builder.Step.Run = b.addSystemCommand(&.{ "python3", "demo.py" });
    demo.setEnvironmentVariable("PYTHONPATH", ".");
    demo.setCwd(.{ .relative = .{ .base = .install_prefix, .sub_path = "python" } });
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
