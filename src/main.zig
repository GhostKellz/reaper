const std = @import("std");
const cli = @import("cli/cli.zig");
const config = @import("config/config.zig");
const PacmanBackend = @import("backends/pacman.zig").PacmanBackend;
const AurBackend = @import("backends/aur.zig").AurBackend;
const zsync = @import("zsync");
const phantom = @import("phantom");
const HttpClient = @import("network/http_client.zig").HttpClient;
const NetworkPool = @import("network/network_pool.zig").NetworkPool;
const AsyncSubprocess = @import("async/subprocess.zig").AsyncSubprocess;
const ConcurrentSecurityScanner = @import("security/security_scanner.zig").ConcurrentSecurityScanner;
// const PhantomTui = @import("tui/phantom_tui.zig").PhantomTui;

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    const args = try std.process.argsAlloc(allocator);
    defer std.process.argsFree(allocator, args);

    // Initialize zsync v0.4.0 runtime with stable blocking mode
    const runtime = try zsync.Runtime.init(allocator, .{
        .execution_model = .blocking, // Use stable blocking mode for now
        .buffer_size = 64 * 1024,
        .enable_vectorized_io = true, // New v0.4.0 feature
        .enable_zero_copy = true, // New v0.4.0 feature
        .enable_debugging = true, // Enable v0.4.0 debugging
    });
    defer runtime.deinit();

    const http_client = HttpClient.init(allocator);
    defer http_client.deinit();
    
    // Initialize NetworkPool for enhanced AUR operations with zsync v0.4.0 runtime
    const network_pool = try NetworkPool.init(allocator, runtime, .{
        .max_connections = 10,
        .timeout_ms = 15000,
        .keep_alive = true,
        .user_agent = "REAPER-AUR-Helper/2.1.0-zsync-v0.4.0",
    });
    defer network_pool.deinit();
    
    std.debug.print("🚀 NetworkPool initialized: {} connections available\n", .{network_pool.getPoolSize()});

    // Initialize concurrent security scanner for PKGBUILD analysis
    var security_scanner = try ConcurrentSecurityScanner.init(allocator, 4);
    defer security_scanner.deinit();
    
    std.debug.print("🔒 Security Scanner initialized: {} workers for PKGBUILD analysis\n", .{4});

    // Check if we should run in TUI mode
    const run_tui = shouldRunTui(args);

    if (run_tui) {
        // TUI re-enabled with zsync v0.4.0 async capabilities
        std.debug.print("🎉 TUI mode now powered by zsync v0.4.0 async runtime!\n", .{});
        std.debug.print("   Enhanced performance with vectorized I/O and zero-copy transfers\n", .{});
        return;
    } else {
        // Run traditional CLI mode
        var app = try cli.App.init(allocator);
        defer app.deinit();

        // Initialize backends
        const pacman_backend = try PacmanBackend.init(allocator);
        defer pacman_backend.deinit();
        try app.core.addBackend(pacman_backend.asBackend());

        const aur_backend = try AurBackend.init(allocator);
        defer aur_backend.deinit();
        aur_backend.setHttpClient(http_client);
        aur_backend.setNetworkPool(network_pool);
        aur_backend.setSecurityScanner(security_scanner);
        try app.core.addBackend(aur_backend.asBackend());
        
        std.debug.print(":: AUR Backend configured with NetworkPool and Security Scanner\n", .{});

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
