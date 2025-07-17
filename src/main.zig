const std = @import("std");
const cli = @import("cli/cli.zig");
const config = @import("config/config.zig");
const PacmanBackend = @import("backends/pacman.zig").PacmanBackend;
const AurBackend = @import("backends/aur.zig").AurBackend;
const zsync = @import("zsync");
const phantom = @import("phantom");
const HttpClient = @import("network/http_client.zig").HttpClient;
const AsyncSubprocess = @import("async/subprocess.zig").AsyncSubprocess;
const PhantomTui = @import("tui/phantom_tui.zig").PhantomTui;

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    const args = try std.process.argsAlloc(allocator);
    defer std.process.argsFree(allocator, args);

    // Initialize zsync runtime with enhanced configuration
    var runtime = try zsync.Runtime.init(allocator, .{
        .thread_pool_size = 8, // Increased for better parallel performance
        .max_tasks = 2048, // More concurrent tasks
        .enable_io = true,
        .enable_timers = true,
    });
    defer runtime.deinit();

    // Initialize custom HTTP client
    var http_client = HttpClient.init(allocator);
    defer http_client.deinit();

    // Create async subprocess handler
    var async_subprocess = AsyncSubprocess.init(allocator, runtime);

    // Check if we should run in TUI mode
    const run_tui = shouldRunTui(args);

    if (run_tui) {
        // Initialize and run phantom TUI
        var phantom_tui = try PhantomTui.init(allocator, runtime, http_client);
        defer phantom_tui.deinit();

        try phantom_tui.run();
    } else {
        // Run traditional CLI mode
        var app = try cli.App.init(allocator);
        defer app.deinit();

        // Initialize backends with new async capabilities
        const pacman_backend = try PacmanBackend.init(allocator);
        defer pacman_backend.deinit();
        pacman_backend.setAsyncSubprocess(&async_subprocess);
        // TODO: Add setHttpClient method to PacmanBackend if needed
        try app.core.addBackend(pacman_backend.asBackend());

        const aur_backend = try AurBackend.init(allocator);
        defer aur_backend.deinit();
        aur_backend.setHttpClient(http_client);
        try app.core.addBackend(aur_backend.asBackend());

        try app.run(args);
    }
}

fn shouldRunTui(args: [][:0]u8) bool {
    // Run TUI mode if no arguments provided or --tui flag is present
    if (args.len == 1) return true; // Only program name, no other args

    for (args[1..]) |arg| {
        if (std.mem.eql(u8, arg, "--tui") or std.mem.eql(u8, arg, "-t")) {
            return true;
        }
        if (std.mem.eql(u8, arg, "--no-tui")) {
            return false;
        }
    }

    // Default to CLI mode if specific commands are provided
    return false;
}
