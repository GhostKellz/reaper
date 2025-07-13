const std = @import("std");
const iso = @import("iso.zig");
const kernel = @import("kernel.zig");
const ghostnv = @import("ghostnv.zig");

pub fn main(allocator: std.mem.Allocator, args: []const []const u8) !void {
    if (args.len == 0) {
        printHelp();
        return;
    }
    
    const target = args[0];
    const target_args = if (args.len > 1) args[1..] else &[_][]const u8{};
    
    if (std.mem.eql(u8, target, "iso")) {
        try iso.buildIso(allocator, target_args);
    } else if (std.mem.eql(u8, target, "kernel")) {
        try kernel.buildKernel(allocator, target_args);
    } else if (std.mem.eql(u8, target, "ghostnv")) {
        try ghostnv.buildGhostNv(allocator, target_args);
    } else if (std.mem.eql(u8, target, "--help") or std.mem.eql(u8, target, "-h")) {
        printHelp();
    } else {
        std.debug.print("Unknown target: {s}\n", .{target});
        printHelp();
        return error.UnknownTarget;
    }
}

fn printHelp() void {
    std.debug.print(
        \\reap make - Advanced builder for kernels, drivers, and ISOs
        \\
        \\Usage: reap make <target> [options]
        \\
        \\Targets:
        \\  kernel     Build custom Linux kernel with patches
        \\  ghostnv    Build NVIDIA Ghost Open Driver  
        \\  iso        Create Arch-based ISO with custom stack
        \\
        \\Examples:
        \\  reap make kernel --profile amd-x3d
        \\  reap make ghostnv --kernel linux-ghost
        \\  reap make iso --profile gaming --include ghostnv
        \\
        \\Run 'reap make <target> --help' for target-specific options
        \\
    , .{});
}