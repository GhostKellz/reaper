const std = @import("std");
const cli = @import("cli/cli.zig");
const config = @import("config/config.zig");
const PacmanBackend = @import("backends/pacman.zig").PacmanBackend;
const AurBackend = @import("backends/aur.zig").AurBackend;
const zsync = @import("zsync");
const AsyncSubprocess = @import("async/subprocess.zig").AsyncSubprocess;

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    const args = try std.process.argsAlloc(allocator);
    defer std.process.argsFree(allocator, args);

    // Initialize zsync runtime
    var runtime = try zsync.Runtime.init(allocator, .{
        .thread_pool_size = 4,
        .max_tasks = 1024,
        .enable_io = true,
        .enable_timers = true,
    });
    defer runtime.deinit();

    // Create async subprocess handler
    var async_subprocess = AsyncSubprocess.init(allocator, &runtime);

    var app = try cli.App.init(allocator);
    defer app.deinit();

    // Initialize backends
    const pacman_backend = try PacmanBackend.init(allocator);
    defer pacman_backend.deinit();
    pacman_backend.setAsyncSubprocess(&async_subprocess);
    try app.core.addBackend(pacman_backend.asBackend());

    const aur_backend = try AurBackend.init(allocator);
    defer aur_backend.deinit();
    try app.core.addBackend(aur_backend.asBackend());

    try app.run(args);
}
