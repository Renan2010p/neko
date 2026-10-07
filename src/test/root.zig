//! Unit-test root for `zig build test`.
//!
//! Importing a module here runs every `test` block it declares. Only pure,
//! backend-free modules belong here; anything that needs a window is not
//! unit-testable.

test {
    _ = @import("../core/base/types.zig");
    _ = @import("../core/math.zig");
    _ = @import("../core/math/scalar.zig");
    _ = @import("../core/math/vec2.zig");
    _ = @import("../core/math/vec3.zig");
    _ = @import("../core/math/mat4.zig");
    _ = @import("../core/3d/mesh.zig");
    _ = @import("../core/3d/camera.zig");
    _ = @import("../core/system/random.zig");
    _ = @import("../core/system/localization.zig");
    _ = @import("../core/system/save.zig");
    _ = @import("../core/scene/scene.zig");
    _ = @import("../core/scene/script.zig");
    _ = @import("../core/scene/timer.zig");
}
