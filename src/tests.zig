//! Unit-test root for `zig build test`.
//!
//! Importing a module here runs every `test` block it declares. Only pure,
//! backend-free modules belong here; anything that needs a window is exercised
//! by the examples instead.

test {
    _ = @import("core/types.zig");
    _ = @import("core/math.zig");
    _ = @import("core/system/random.zig");
    _ = @import("core/system/localization.zig");
    _ = @import("core/system/save.zig");
    _ = @import("core/scene/scene.zig");
    _ = @import("core/scene/script.zig");
    _ = @import("core/scene/timer.zig");
}
