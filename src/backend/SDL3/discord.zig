// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Renan Lucas Vieira Hilário

//! A minimal Discord Rich Presence client for the hosted SDL2 backends.
//!
//! It speaks the Discord IPC protocol over the local socket (Linux:
//! `/run/user/<uid>/discord-ipc-N`, falling back to `/tmp/discord-ipc-N`). Every
//! failure is swallowed: when Discord is not running the calls are no-ops.

const std: type = @import("std");
const builtin: type = @import("builtin");
const engine: type = @import("neko");

/// A best-effort Discord IPC connection.
pub const Client: type = struct {
    io: ?std.Io = null,
    stream: ?std.Io.net.Stream = null,
    nonce: u32 = 0,

    pub fn connect(self: *Client, io: ?std.Io, client_id: []const u8) bool {
        const io_v: std.Io = io orelse return false;
        self.io = io_v;
        if (self.stream != null) return true;
        if (!self.openSocket(io_v)) return false;

        var msg: [512]u8 = undefined;
        const json: []const u8 = std.fmt.bufPrint(&msg, "{{\"v\":1,\"client_id\":\"{s}\"}}", .{client_id}) catch {
            self.close();
            return false;
        };
        self.sendFrame(0, json);
        // Discord replies immediately (READY or CLOSE). Treat a refusal as a
        // failed connection so `connected()` and the logs are honest.
        if (!self.readReply()) {
            self.close();
            return false;
        }
        return true;
    }

    pub fn set(self: *Client, presence: engine.DiscordPresence) bool {
        if (self.stream == null) return false;
        var actbuf: [1024]u8 = undefined;
        const activity: []const u8 = buildActivity(&actbuf, presence);
        var jsonbuf: [1600]u8 = undefined;
        self.nonce +%= 1;
        const json: []const u8 = std.fmt.bufPrint(
            &jsonbuf,
            "{{\"cmd\":\"SET_ACTIVITY\",\"args\":{{\"pid\":{d},\"activity\":{s}}},\"nonce\":\"{d}\"}}",
            .{ processId(), activity, self.nonce },
        ) catch return false;
        engine.log.debug("discord: -> {s}", .{json});
        self.sendFrame(1, json);
        return self.readReply();
    }

    pub fn clear(self: *Client) void {
        if (self.stream == null) return;
        self.nonce +%= 1;
        var buf: [128]u8 = undefined;
        const json: []const u8 = std.fmt.bufPrint(
            &buf,
            "{{\"cmd\":\"SET_ACTIVITY\",\"args\":{{\"pid\":{d},\"activity\":null}},\"nonce\":\"{d}\"}}",
            .{ processId(), self.nonce },
        ) catch return;
        self.sendFrame(1, json);
    }

    pub fn close(self: *Client) void {
        if (self.io) |io| {
            if (self.stream) |stream| stream.close(io);
        }
        self.stream = null;
        self.io = null;
    }

    pub fn isConnected(self: *Client) bool {
        return self.stream != null;
    }

    // ── internals ────────────────────────────────────────────────────────

    fn openSocket(self: *Client, io: std.Io) bool {
        var buf: [160]u8 = undefined;
        const uid: u32 = currentUid();
        var i: usize = 0;
        while (i < 10) : (i += 1) {
            // Native, Flatpak, Snap and /tmp locations, in that order.
            if (self.tryPath(io, &buf, "/run/user/{d}/discord-ipc-{d}", .{ uid, i })) return true;
            if (self.tryPath(io, &buf, "/tmp/discord-ipc-{d}", .{i})) return true;
            if (self.tryPath(io, &buf, "/run/user/{d}/.flatpak/com.discordapp.Discord/xdg-run/discord-ipc-{d}", .{ uid, i })) return true;
            if (self.tryPath(io, &buf, "/run/user/{d}/app/com.discordapp.Discord/discord-ipc-{d}", .{ uid, i })) return true;
            if (self.tryPath(io, &buf, "/run/user/{d}/snap.discord/discord-ipc-{d}", .{ uid, i })) return true;
        }
        engine.log.warn("discord: no IPC socket found (is Discord running?)", .{});
        return false;
    }

    fn readExact(self: *Client, io: std.Io, stream: std.Io.net.Stream, buf: []u8) bool {
        _ = self;
        var off: usize = 0;
        while (off < buf.len) {
            var dst: [1][]u8 = .{buf[off..]};
            const result = io.operate(.{ .net_read = .{
                .socket_handle = stream.socket.handle,
                .data = dst[0..],
            } }) catch return false;
            const rr = result.net_read catch return false;
            const n: usize = rr.data_len;
            if (n == 0) return false;
            off += n;
        }
        return true;
    }

    /// Reads one frame and logs it. Returns false when Discord refused.
    fn readReply(self: *Client) bool {
        const io: std.Io = self.io orelse return false;
        const stream: std.Io.net.Stream = self.stream orelse return false;
        var header: [8]u8 = undefined;
        if (!self.readExact(io, stream, &header)) return false;
        const opcode: u32 = std.mem.readInt(u32, header[0..4], .little);
        const length: u32 = std.mem.readInt(u32, header[4..8], .little);
        var buf: [512]u8 = undefined;
        const n: usize = @min(@as(usize, length), buf.len);
        if (n > 0 and !self.readExact(io, stream, buf[0..n])) return false;
        const shown: []const u8 = buf[0..@min(n, 160)];
        if (opcode == 2) {
            engine.log.err("discord: refused: {s}", .{shown});
            return false;
        }
        engine.log.debug("discord: <- op={d} {s}", .{ opcode, shown });
        return true;
    }

    fn tryPath(self: *Client, io: std.Io, buf: []u8, comptime fmt: []const u8, args: anytype) bool {
        const path: []const u8 = std.fmt.bufPrint(buf, fmt, args) catch return false;
        const address = std.Io.net.UnixAddress.init(path) catch return false;
        const stream = address.connect(io) catch return false;
        self.stream = stream;
        engine.log.info("discord: connected to {s}", .{path});
        return true;
    }

    fn sendFrame(self: *Client, opcode: u32, json: []const u8) void {
        const io: std.Io = self.io orelse return;
        const stream: std.Io.net.Stream = self.stream orelse return;
        var op_buf: [4]u8 = undefined;
        var len_buf: [4]u8 = undefined;
        std.mem.writeInt(u32, &op_buf, opcode, .little);
        std.mem.writeInt(u32, &len_buf, @intCast(@min(json.len, std.math.maxInt(u32))), .little);
        writeAll(io, stream, &op_buf);
        writeAll(io, stream, &len_buf);
        writeAll(io, stream, json);
    }

    fn writeAll(io: std.Io, stream: std.Io.net.Stream, bytes: []const u8) void {
        var off: usize = 0;
        while (off < bytes.len) {
            const result = io.operate(.{ .net_write = .{
                .socket_handle = stream.socket.handle,
                .data = &.{bytes[off..]},
                .splat = 1,
            } }) catch return;
            const n: usize = result.net_write catch return;
            if (n == 0) return;
            off += n;
        }
    }
};

fn currentUid() u32 {
    if (builtin.os.tag == .linux) return std.os.linux.getuid();
    return 1000;
}

fn processId() u32 {
    if (builtin.os.tag == .linux) return @intCast(std.os.linux.getpid());
    return 0;
}

const SB: type = struct {
    buf: []u8,
    len: usize = 0,

    fn add(self: *SB, s: []const u8) void {
        if (self.len + s.len > self.buf.len) return;
        @memcpy(self.buf[self.len .. self.len + s.len], s);
        self.len += s.len;
    }

    fn strField(self: *SB, first: *bool, key: []const u8, value: []const u8) void {
        if (!first.*) self.add(",");
        first.* = false;
        self.add("\"");
        self.add(key);
        self.add("\":\"");
        self.add(value);
        self.add("\"");
    }

    fn numField(self: *SB, first: *bool, key: []const u8, value: i64) void {
        if (!first.*) self.add(",");
        first.* = false;
        self.add("\"");
        self.add(key);
        self.add("\":");
        const s: []const u8 = std.fmt.bufPrint(self.buf[self.len..], "{d}", .{value}) catch return;
        self.len += s.len;
    }
};

fn buildActivity(buf: []u8, p: engine.DiscordPresence) []const u8 {
    var b: SB = .{ .buf = buf };
    b.add("{");
    var first: bool = true;
    if (p.details.len > 0) b.strField(&first, "details", p.details);
    if (p.state.len > 0) b.strField(&first, "state", p.state);

    if (p.start_timestamp > 0 or p.end_timestamp > 0) {
        if (!first) b.add(",");
        first = false;
        b.add("\"timestamps\":{");
        var f: bool = true;
        if (p.start_timestamp > 0) b.numField(&f, "start", p.start_timestamp);
        if (p.end_timestamp > 0) b.numField(&f, "end", p.end_timestamp);
        b.add("}");
    }

    if (p.large_image.len > 0 or p.large_text.len > 0 or p.small_image.len > 0 or p.small_text.len > 0) {
        if (!first) b.add(",");
        first = false;
        b.add("\"assets\":{");
        var f: bool = true;
        if (p.large_image.len > 0) b.strField(&f, "large_image", p.large_image);
        if (p.large_text.len > 0) b.strField(&f, "large_text", p.large_text);
        if (p.small_image.len > 0) b.strField(&f, "small_image", p.small_image);
        if (p.small_text.len > 0) b.strField(&f, "small_text", p.small_text);
        b.add("}");
    }

    if (p.party_size > 0 and p.party_max > 0) {
        if (!first) b.add(",");
        first = false;
        b.add("\"party\":{\"size\":[");
        const s: []const u8 = std.fmt.bufPrint(b.buf[b.len..], "{d},{d}", .{ p.party_size, p.party_max }) catch return b.buf[0..b.len];
        b.len += s.len;
        b.add("]}");
    }

    b.add("}");
    return b.buf[0..b.len];
}
