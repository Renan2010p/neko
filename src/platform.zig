//! The platform seam — the **only** core file that names the concrete backend.
//!
//! Neko keeps a rigorous split:
//!
//!   - `src/core/**` uses only the abstract `Backend` interface (events,
//!     drawing, textures, text, sound, files) and never touches an OS API.
//!   - `src/platform/<name>/**` is where the real work happens: SDL2 on the
//!     desktop, gsKit on the PS2, and so on.
//!   - this file (`src/platform.zig`) is the single bridge between the two.
//!     It re-exports the backend kind and hands the core an abstract handle,
//!     without leaking the concrete `Engine` type.
//!
//! The build wires the backend module under the stable import name
//! `neko_backend`, so a game never sees it. Switching backends changes the
//! implementation behind `create()` and nothing else in `src/core/**`.

const backend_contract: type = @import("core/backend.zig");
const types: type = @import("core/types.zig");

/// The concrete, build-selected backend module. `neko_backend` is provided by
/// `build.zig`; the core reaches it only through this file.
const backend: type = @import("neko_backend");

/// The abstract backend interface every platform implements.
pub const Backend: type = backend_contract.Backend;

/// Which backend this engine build was compiled with.
pub const kind: types.BackendKind = backend.kind;

/// Returns the active backend as an abstract handle. The concrete engine lives
/// inside the backend module (a process-wide instance), so the core only ever
/// holds a `Backend` (pointer + vtable).
pub fn create() Backend {
    return backend.create();
}
