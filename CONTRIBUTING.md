# Contributing to Neko

Thanks for helping! This is a small engine, so the rules are short.

## Requirements

- **Zig 0.17.0** (`zig version` must print `0.17.0`).
- For the desktop backends, **SDL2 or SDL3** with `_ttf` / `_image` / `_mixer`
  development packages.
- **Python 3** with development headers, only for `zig build python`.

## Build and test

```sh
zig build                     # build the engine modules
zig build test                # unit tests + the freestanding guard
zig build check-freestanding  # fail if src/core touches an OS API
zig build check-targets       # cross-compile the core for many CPU/OS targets
zig build backends            # list the available backends
zig build docs                # API reference -> zig-out/docs/api
zig build python              # the _neko CPython extension + pygame package
```

## Project layout

- `src/core/**` — the engine; platform-agnostic and OS-free.
- `src/backend/<Name>/**` — one backend per platform (`SDL2`, `SDL3`, `PS2`, `PSX`, `Headless`).
- `bindings/python/**` — the CPython extension and the `pygame` package.
- `docs/` — the [guide](docs/guide), [architecture](docs/architecture) and
  [reference](docs/reference), plus [screenshots](docs/screenshots).

## Style

- Run `zig fmt build.zig build.zig.zon src bindings tools` before committing —
  CI checks it.
- 4-space indentation, `snake_case` functions, `SomeType` types, `.fields`.
- Prefer explicit `: type` annotations on declarations in the core.
- English identifiers and comments.
- Keep `src/core/**` free of OS calls: it may only import the abstract
  `Backend` (through `src/core/platform.zig`). Do not import `neko_backend`
  elsewhere.

## Adding a backend

Backends are plugins: create `src/backend/<Name>/` (its `render/<api>/build.zig`
and, for a new kind, an entry in `src/backend/registry.zig`). `build.zig` never
changes. The full contract is in
[docs/architecture/backends.md](docs/architecture/backends.md).

## Pull requests

- Keep changes focused; one topic per PR.
- Update the docs under `docs/` and `CHANGELOG.md` when behaviour changes.
- Make sure `zig build test` and `zig fmt --check` are green.

## License

By contributing you agree to license your work under **GPL-3.0-or-later**
(see [LICENSE](LICENSE)).
