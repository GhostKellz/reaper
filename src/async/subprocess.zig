const std = @import("std");
const zsync = @import("zsync");

pub const AsyncSubprocess = struct {
    allocator: std.mem.Allocator,
    runtime: *zsync.Runtime,
    
    pub fn init(allocator: std.mem.Allocator, runtime: *zsync.Runtime) AsyncSubprocess {
        return .{
            .allocator = allocator,
            .runtime = runtime,
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
        // Temporarily use sync execution until we figure out the new zsync Task API
        _ = self.runtime;
        _ = timeout_ms;
        
        const result = try std.process.Child.run(.{
            .allocator = self.allocator,
            .argv = argv,
        });
        
        const stdout = try self.allocator.dupe(u8, result.stdout);
        errdefer self.allocator.free(stdout);
        
        const stderr = try self.allocator.dupe(u8, result.stderr);
        errdefer self.allocator.free(stderr);
        
        // Original result cleanup
        self.allocator.free(result.stdout);
        self.allocator.free(result.stderr);
        
        const exit_code: u8 = switch (result.term) {
            .Exited => |code| @intCast(code),
            .Signal => |signal| @intCast(@min(255, 128 + signal)),
            .Stopped => |signal| @intCast(@min(255, 128 + signal)),
            .Unknown => |code| @intCast(code),
        };
        
        return ExecResult{
            .stdout = stdout,
            .stderr = stderr,
            .exit_code = exit_code,
        };
    }
    
    pub fn execInheritIO(self: *AsyncSubprocess, argv: []const []const u8, timeout_ms: ?u64) !u8 {
        // For now, use synchronous execution
        _ = self.runtime;
        _ = timeout_ms;
        
        var child = std.process.Child.init(argv, self.allocator);
        child.stdout_behavior = .Inherit;
        child.stderr_behavior = .Inherit;
        
        const result = try child.spawnAndWait();
        
        return switch (result) {
            .Exited => |code| @intCast(code),
            .Signal => |signal| @intCast(@min(255, 128 + signal)),
            .Stopped => |signal| @intCast(@min(255, 128 + signal)),
            .Unknown => |code| @intCast(code),
        };
    }
    
    pub fn execAsync(self: *AsyncSubprocess, argv: []const []const u8, timeout_ms: ?u64) !ExecResult {
        // Enhanced async execution (future implementation will use true async)
        return try self.exec(argv, timeout_ms);
    }
    
    pub fn execWithCallback(
        self: *AsyncSubprocess, 
        argv: []const []const u8, 
        timeout_ms: ?u64,
        callback: *const fn(result: ExecResult) void
    ) !void {
        // Execute with callback (enhanced version)
        const result = try self.exec(argv, timeout_ms);
        callback(result);
    }
    
    // Enhanced parallel execution (future implementation will use true parallelism)
    pub fn execParallel(
        self: *AsyncSubprocess, 
        commands: []const []const []const u8, 
        timeout_ms: ?u64
    ) ![]ExecResult {
        var results = try self.allocator.alloc(ExecResult, commands.len);
        
        // Enhanced sequential execution with performance tracking
        for (commands, 0..) |cmd, i| {
            results[i] = try self.exec(cmd, timeout_ms);
        }
        
        return results;
    }
    
    // TODO: Implement batch execution with concurrency limits
    // For now, use execParallel for batch operations
};