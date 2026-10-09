# Contributing to Neko

Thanks for helping! This is a small engine, so the rules are short.

## Requirements

- **Zig 0.16.0** (`zig version` must print `0.16.0`; 0.17.0 is not supported
  yet — it removed `std.Build` helpers Neko uses).
- **SDL2** + `SDL2_ttf` / `SDL2_image` / `SDL2_mixer` for the desktop backend.
- **Python 3** with development headers, only for `zig build python`.

## Build and test

```sh
zig build            # build the engine modules
zig build test       # unit tests (fast, backend-free)
zig build backends   # list the available backends
zig build docs       # API reference -> zig-out/docs/api
zig build python     # the _neko CPython extension + pygame package
```

## Style

- Run `zig fmt build.zig build src bindings` before committing. CI checks it.
- 4-space indentation, snake_case functions, `SomeType` types, `.fields`.
- Keep `src/core/**` free of OS calls: it may only import the abstract
  `Backend` (through `src/core/platform.zig`). Do not import `neko_backend`
  elsewhere.
- Prefer explicit types on `const`/`var` in the core where it aids clarity.

## Adding a backend

Backends are plugins: create `src/backend/<Name>/` (its `render/<api>/build.zig`
and, for a new kind, an entry in `src/backend/registry.zig`). `build.zig` never
changes. The full
contract is in [docs/backends.md](docs/architecture/backends.md).

## Pull requests

- Keep changes focused; one topic per PR.
- Update the docs under `docs/` and `CHANGELOG.md` when behaviour changes.
- Make sure `zig build test` and `zig fmt --check` are green.

## License

By contributing you agree to license your work under **GPL-3.0-or-later**
(see [LICENSE](LICENSE)).
