//! `neko.localization` — a tiny key/text table for translations.
//!
//! Games register their tables and pick a language; lookups fall back to the
//! key itself when nothing matches, so a missing string is never a crash.

const std: type = @import("std");
const mem: type = std.mem;

/// One translation entry.
pub const Entry: type = struct {
    key: []const u8,
    value: []const u8,
};

/// All entries for one language.
pub const Table: type = struct {
    lang: []const u8,
    entries: []const Entry,
};

var tables: []const Table = &.{};
var current: []const u8 = "pt";

/// Registers the available languages.
pub fn load(available: []const Table) void {
    tables = available;
}

/// Selects `lang` if it is available; otherwise keeps the current one.
pub fn set_language(lang: []const u8) void {
    for (tables) |t| {
        if (mem.eql(u8, t.lang, lang)) {
            current = lang;
            return;
        }
    }
}

/// The selected language.
pub fn language() []const u8 {
    return current;
}

/// Looks up `key` in the current language, falling back to `key`.
pub fn text(key: []const u8) []const u8 {
    for (tables) |t| {
        if (!mem.eql(u8, t.lang, current)) continue;
        for (t.entries) |e| {
            if (mem.eql(u8, e.key, key)) return e.value;
        }
    }
    return key;
}

// ── Tests ────────────────────────────────────────────────────────────────────

const testing: type = @import("std").testing;

const en_entries: [2]Entry = [_]Entry{
    .{ .key = "hello", .value = "Hello" },
    .{ .key = "bye", .value = "Bye" },
};
const pt_entries: [1]Entry = [_]Entry{
    .{ .key = "hello", .value = "Ola" },
};
const test_tables: [2]Table = [_]Table{
    .{ .lang = "en", .entries = &en_entries },
    .{ .lang = "pt", .entries = &pt_entries },
};

test "localization: lookup, fallback and language switch" {
    load(&test_tables);
    set_language("en");
    try testing.expectEqualStrings("Hello", text("hello"));
    try testing.expectEqualStrings("missing", text("missing"));

    set_language("pt");
    try testing.expectEqualStrings("Ola", text("hello"));
    try testing.expectEqualStrings("bye", text("bye")); // not in pt -> key

    set_language("xx"); // unknown -> keeps the current language
    try testing.expectEqualStrings("pt", language());
}
