const std = @import("std");

pub const AsyncSubprocess = struct {
    allocator: std.mem.Allocator,
    
    pub fn init(allocator: std.mem.Allocator, runtime: anytype) AsyncSubprocess {
        _ = runtime; // Will use runtime for actual async later
        return .{
            .allocator = allocator,
        };
    }
    
    pub const ExecResult = struct {
        stdout: []const u8,
        stderr: []const u8,
        exit_code: u8,
        
        pub fn deinit(self: ExecResult, allocator: std.mem.Allocator) void {
            allocator.free(self.stdout);
            allocator.free(self.stderr);
        }
    };
    
    pub fn exec(self: *AsyncSubprocess, argv: []const []const u8, timeout_ms: ?u64) !ExecResult {
        var child = std.process.Child.init(argv, self.allocator);
        child.stdout_behavior = .Pipe;
        child.stderr_behavior = .Pipe;
        
        try child.spawn();
        
        // For now, use a thread to handle timeout
        const term = if (timeout_ms) |timeout| blk: {
            const WaitContext = struct {
                child: *std.process.Child,
                result: ?std.process.Child.Term = null,
                done: std.atomic.Value(bool) = std.atomic.Value(bool).init(false),
            };
            
            var ctx = WaitContext{ .child = &child };
            
            const wait_thread = try std.Thread.spawn(.{}, struct {
                fn wait(context: *WaitContext) void {
                    context.result = context.child.wait() catch null;
                    context.done.store(true, .release);
                }
            }.wait, .{&ctx});
            
            const start_time = std.time.milliTimestamp();
            while (!ctx.done.load(.acquire)) {
                const elapsed = std.time.milliTimestamp() - start_time;
                if (elapsed >= timeout) {
                    _ = try child.kill();
                    wait_thread.join();
                    return error.ProcessTimeout;
                }
                std.time.sleep(10 * std.time.ns_per_ms);
            }
            
            wait_thread.join();
            break :blk ctx.result orelse return error.ProcessFailed;
        } else try child.wait();
        
        // Read output
        const stdout = try child.stdout.?.reader().readAllAlloc(self.allocator, 1024 * 1024);
        errdefer self.allocator.free(stdout);
        
        const stderr = try child.stderr.?.reader().readAllAlloc(self.allocator, 1024 * 1024);
        errdefer self.allocator.free(stderr);
        
        return .{
            .stdout = stdout,
            .stderr = stderr,
            .exit_code = switch (term) {
                .Exited => |code| code,
                else => 255,
            },
        };
    }
    
    pub fn execInheritIO(self: *AsyncSubprocess, argv: []const []const u8, timeout_ms: ?u64) !u8 {
        var child = std.process.Child.init(argv, self.allocator);
        child.stdout_behavior = .Inherit;
        child.stderr_behavior = .Inherit;
        
        try child.spawn();
        
        // For now, use a thread to handle timeout
        const term = if (timeout_ms) |timeout| blk: {
            const WaitContext = struct {
                child: *std.process.Child,
                result: ?std.process.Child.Term = null,
                done: std.atomic.Value(bool) = std.atomic.Value(bool).init(false),
            };
            
            var ctx = WaitContext{ .child = &child };
            
            const wait_thread = try std.Thread.spawn(.{}, struct {
                fn wait(context: *WaitContext) void {
                    context.result = context.child.wait() catch null;
                    context.done.store(true, .release);
                }
            }.wait, .{&ctx});
            
            const start_time = std.time.milliTimestamp();
            while (!ctx.done.load(.acquire)) {
                const elapsed = std.time.milliTimestamp() - start_time;
                if (elapsed >= timeout) {
                    _ = try child.kill();
                    wait_thread.join();
                    return error.ProcessTimeout;
                }
                std.time.sleep(10 * std.time.ns_per_ms);
            }
            
            wait_thread.join();
            break :blk ctx.result orelse return error.ProcessFailed;
        } else try child.wait();
        
        return switch (term) {
            .Exited => |code| code,
            else => 255,
        };
    }
};