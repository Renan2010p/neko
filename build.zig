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
//! A game depends on this package and picks a backend:
//!
//!     b.dependency("neko", .{ .backend = .sdl2 });
//!
//! ## Steps
//!
//! From this repository you can run:
//!
//!   - `zig build`            build the engine modules (no artifacts)
//!   - `zig build examples`   compile every example under `examples/`
//!   - `zig build run-hello`  build and run one example (needs a display)
//!   - `zig build test`       run the unit tests
//!   - `zig build docs`       emit the API reference to `zig-out/docs/api`
//!
//! The examples and the API docs are only built for the SDL2 backend on a
//! hosted target; the PS2 target is freestanding and has neither a window nor
//! a browser.

const std: type = @import("std");
const builtin: type = std.builtin;
const Builder: type = std.Build;

/// Backends this package can provide. Add new ones here.
pub const Backend: type = enum {
    sdl2,
    sdl3,
    ps2,
};

pub fn build(b: *Builder) void {
    const target: Builder.ResolvedTarget = b.standardTargetOptions(.{});
    // Games are performance-sensitive, so an unqualified `zig build` uses
    // ReleaseFast. Override with `-Doptimize=Debug` (or Safe/Small/ReleaseFast).
    const optimize: builtin.OptimizeMode = b.option(
        builtin.OptimizeMode,
        "optimize",
        "Prioritize performance, safety, or binary size",
    ) orelse .ReleaseFast;

    // The consumer (a game) chooses the backend through its dependency
    // arguments, e.g. `.backend = .sdl2`.
    const backend: Backend = b.option(
        Backend,
        "backend",
        "Backend to build: sdl2 (default), sdl3, ps2",
    ) orelse .sdl2;

    // ── Engine module ────────────────────────────────────────────────────
    // Always built. Game code imports this as `neko`.
    // `link_libc` only makes sense on hosted targets; the PS2 is freestanding
    // and links the PS2SDK libc from C instead.
    const hosted: bool = target.result.os.tag != .freestanding;

    const neko: *Builder.Module = b.addModule("neko", .{
        .root_source_file = b.path("src/neko.zig"),
        .target = target,
        .optimize = optimize,
        .link_libc = hosted,
    });

    // ── pygame compatibility module ──────────────────────────────────────
    // A pygame-shaped software layer (Surface/draw/transform/display) built
    // only on neko's public API. Games import it as `neko_pygame`.
    const pygame: *Builder.Module = b.addModule("neko_pygame", .{
        .root_source_file = b.path("src/compat/pygame.zig"),
        .target = target,
        .optimize = optimize,
        .link_libc = hosted,
        .imports = &.{.{ .name = "neko", .module = neko }},
    });

    // ── Backend module ───────────────────────────────────────────────────
    // Published under the stable name `neko_backend`, so a game imports the
    // same module no matter which backend was selected.
    const backend_module: *Builder.Module = switch (backend) {
        .sdl2 => build_sdl2(b, neko, target, optimize),
        .sdl3 => @panic("neko: the SDL3 backend is not implemented yet"),
        .ps2 => build_ps2(b, neko, target, optimize),
    };

    // The engine's `screen` module instantiates the selected backend, so the
    // engine module imports the backend too. Cyclical module imports are
    // allowed in Zig.
    neko.addImport("neko_backend", backend_module);

    // ── Developer steps ──────────────────────────────────────────────────
    // Meaningful only on a hosted target: a freestanding build has no window
    // and cannot run a test executable, and the API docs need a hosted object.
    if (hosted) {
        add_examples(b, neko, pygame, backend, target, optimize);
        add_tests(b, target, optimize);
        add_docs(b, neko);
        add_python(b, neko, pygame, target, optimize);
    }
}

// ── Backend wiring ───────────────────────────────────────────────────────────

/// Builds the SDL2 backend and publishes it as `neko_backend`.
fn build_sdl2(
    b: *Builder,
    neko: *Builder.Module,
    target: Builder.ResolvedTarget,
    optimize: builtin.OptimizeMode,
) *Builder.Module {
    const c_translate: *Builder.Step.TranslateC = b.addTranslateC(.{
        .root_source_file = b.path("src/platform/sdl2/SDL2.h"),
        .optimize = optimize,
        .target = target,
        .link_libc = true,
    });
    c_translate.linkSystemLibrary("sdl2", .{});
    c_translate.linkSystemLibrary("SDL2_ttf", .{});
    c_translate.linkSystemLibrary("SDL2_image", .{});
    c_translate.linkSystemLibrary("SDL2_mixer", .{});

    const c_module: *Builder.Module = c_translate.createModule();

    const backend: *Builder.Module = b.addModule("neko_backend", .{
        .root_source_file = b.path("src/platform/sdl2/sdl2.zig"),
        .target = target,
        .optimize = optimize,
        .link_libc = true,
        .imports = &.{
            .{ .name = "c", .module = c_module },
            .{ .name = "neko", .module = neko },
        },
    });
    backend.linkSystemLibrary("sdl2", .{});
    backend.linkSystemLibrary("SDL2_ttf", .{});
    backend.linkSystemLibrary("SDL2_image", .{});
    backend.linkSystemLibrary("SDL2_mixer", .{});

    return backend;
}

/// Builds the PS2 backend (gsKit + pad) and publishes it as `neko_backend`.
///
/// Nothing is linked here: the target is freestanding, so the game is compiled
/// with Zig's C backend (`-ofmt=c`) and the emitted C is linked by the PS2DEV
/// toolchain (`mips64r5900el-ps2-elf-gcc`) against gsKit and the PS2SDK. This
/// function only wires the modules together so `neko.screen` finds the backend.
fn build_ps2(
    b: *Builder,
    neko: *Builder.Module,
    target: Builder.ResolvedTarget,
    optimize: builtin.OptimizeMode,
) *Builder.Module {
    return b.addModule("neko_backend", .{
        .root_source_file = b.path("src/platform/ps2/ps2.zig"),
        .target = target,
        .optimize = optimize,
        .imports = &.{
            .{ .name = "neko", .module = neko },
        },
    });
}

// ── Examples ─────────────────────────────────────────────────────────────────

const Example: type = struct {
    /// Step/file base name (`examples/<name>.zig`).
    name: []const u8,
    /// One-line description shown in `zig build --help` and the docs.
    desc: []const u8,
};

/// Every example is a standalone `pub fn main(init: std.process.Init)`.
const examples: [16]Example = [_]Example{
    .{ .name = "hello", .desc = "least boilerplate: neko.run + frame callback" },
    .{ .name = "app", .desc = "the high-level neko.app runner" },
    .{ .name = "window", .desc = "an explicit Window object + switch(event)" },
    .{ .name = "scenes", .desc = "a window with switchable scenes (switch_to)" },
    .{ .name = "script", .desc = "attach a plain object to the scene as a node" },
    .{ .name = "splash", .desc = "the NEKO boot splash (asset-free)" },
    .{ .name = "shapes", .desc = "draw primitives and text" },
    .{ .name = "input", .desc = "keyboard, mouse and window events" },
    .{ .name = "sprites", .desc = "named sprites, placeholders and a widget" },
    .{ .name = "scene_tree", .desc = "a Godot-style node tree" },
    .{ .name = "audio", .desc = "load and play a sound" },
    .{ .name = "save", .desc = "binary serialization and save files" },
    .{ .name = "pygame_compat", .desc = "a pygame-shaped Surface/draw layer on Neko" },
    .{ .name = "pygame_screenshot", .desc = "render the compat scene to a PPM (headless)" },
    .{ .name = "vesper_neko", .desc = "Vesper's Nara surface drawn with Neko" },
    .{ .name = "vesper_neko_shot", .desc = "render the Vesper scene to a PPM (headless)" },
};

/// Adds the `examples` step (build all) and one `run-<name>` step per example.
fn add_examples(
    b: *Builder,
    neko: *Builder.Module,
    pygame: *Builder.Module,
    backend: Backend,
    target: Builder.ResolvedTarget,
    optimize: builtin.OptimizeMode,
) void {
    const step: *Builder.Step = b.step("examples", "Build every example under examples/");

    // The only working hosted backend for examples today.
    if (backend != .sdl2) {
        return;
    }

    for (examples) |ex| {
        const module: *Builder.Module = b.createModule(.{
            .root_source_file = b.path(b.fmt("examples/{s}.zig", .{ex.name})),
            .target = target,
            .optimize = optimize,
            .link_libc = true,
            .imports = &.{
                .{ .name = "neko", .module = neko },
                .{ .name = "neko_pygame", .module = pygame },
            },
        });
        link_sdl2(module);

        const exe: *Builder.Step.Compile = b.addExecutable(.{
            .name = ex.name,
            .root_module = module,
        });

        step.dependOn(&exe.step);

        // `zig build run-<name>` builds and runs that example.
        const run: *Builder.Step.Run = b.addRunArtifact(exe);
        if (b.args) |args| run.addArgs(args);
        const run_step: *Builder.Step = b.step(
            b.fmt("run-{s}", .{ex.name}),
            b.fmt("Run the '{s}' example ({s})", .{ ex.name, ex.desc }),
        );
        run_step.dependOn(&run.step);
    }
}

/// Links the SDL2 family against a module. The backend module already declares
/// these, but naming them here keeps the example executables self-contained.
fn link_sdl2(module: *Builder.Module) void {
    module.linkSystemLibrary("sdl2", .{});
    module.linkSystemLibrary("SDL2_ttf", .{});
    module.linkSystemLibrary("SDL2_image", .{});
    module.linkSystemLibrary("SDL2_mixer", .{});
}

// ── Tests ────────────────────────────────────────────────────────────────────

/// Adds `zig build test`, running the unit tests in the core modules.
fn add_tests(
    b: *Builder,
    target: Builder.ResolvedTarget,
    optimize: builtin.OptimizeMode,
) void {
    const module: *Builder.Module = b.createModule(.{
        .root_source_file = b.path("src/tests.zig"),
        .target = target,
        .optimize = optimize,
        .link_libc = true,
    });

    const tests: *Builder.Step.Compile = b.addTest(.{ .root_module = module });
    const run_tests: *Builder.Step.Run = b.addRunArtifact(tests);

    const step: *Builder.Step = b.step("test", "Run the unit tests");
    step.dependOn(&run_tests.step);
}

// ── Python bindings ──────────────────────────────────────────────────────────

/// Adds `zig build python`: the CPython extension `_neko` (in Zig) plus the
/// pure-Python `pygame` package, installed under `zig-out/python/`. Put that
/// directory on `PYTHONPATH` to run pygame games on Neko.
fn add_python(
    b: *Builder,
    neko: *Builder.Module,
    pygame: *Builder.Module,
    target: Builder.ResolvedTarget,
    optimize: builtin.OptimizeMode,
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

    const ext: *Builder.Step.Compile = b.addLibrary(.{
        .linkage = .dynamic,
        .name = "_neko",
        .root_module = ext_module,
    });
    link_sdl2(ext.root_module);
    // A Python extension leaves the CPython symbols unresolved; the running
    // interpreter provides them when the module is imported.
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
