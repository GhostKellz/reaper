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
        // Use zsync async subprocess with timeout
        const subprocess_config = zsync.SubprocessConfig{
            .argv = argv,
            .stdout_behavior = .Pipe,
            .stderr_behavior = .Pipe,
            .timeout_ms = timeout_ms,
        };
        
        const subprocess = try self.runtime.spawnSubprocess(subprocess_config);
        const result = try subprocess.await();
        
        // Read output using zsync async I/O
        const stdout = try result.stdout.readToEndAlloc(self.allocator, 1024 * 1024);
        errdefer self.allocator.free(stdout);
        
        const stderr = try result.stderr.readToEndAlloc(self.allocator, 1024 * 1024);
        errdefer self.allocator.free(stderr);
        
        return .{
            .stdout = stdout,
            .stderr = stderr,
            .exit_code = @intCast(result.status),
        };
    }
    
    pub fn execInheritIO(self: *AsyncSubprocess, argv: []const []const u8, timeout_ms: ?u64) !u8 {
        // Use zsync async subprocess with inherited I/O
        // Use standard subprocess execution for now
        // TODO: Update to use zsync subprocess API when available
        var child = std.process.Child.init(argv, self.allocator);
        child.stdout_behavior = .Inherit;
        child.stderr_behavior = .Inherit;
        
        const result = try child.spawnAndWait();
        _ = timeout_ms; // TODO: Implement timeout
        
        return switch (result) {
            .Exited => |code| code,
            .Signal => |signal| {
                std.log.warn("Process terminated by signal: {}", .{signal});
                return @intCast(@min(255, 128 + signal));
            },
            .Stopped => |signal| {
                std.log.warn("Process stopped by signal: {}", .{signal});
                return @intCast(@min(255, 128 + signal));
            },
            .Unknown => |code| {
                std.log.warn("Process terminated with unknown status: {}", .{code});
                return @intCast(code);
            },
        };
    }
    
    pub fn execAsync(self: *AsyncSubprocess, argv: []const []const u8, timeout_ms: ?u64) !zsync.Task {
        // Return a task that can be awaited later
        return try self.runtime.spawn(struct {
            fn run(async_subprocess: *AsyncSubprocess, args: []const []const u8, timeout: ?u64) !ExecResult {
                return try async_subprocess.exec(args, timeout);
            }
        }.run, .{ self, argv, timeout_ms });
    }
    
    pub fn execWithCallback(
        self: *AsyncSubprocess, 
        argv: []const []const u8, 
        timeout_ms: ?u64,
        callback: *const fn(result: ExecResult) void
    ) !void {
        // Execute asynchronously and call callback when done
        _ = try self.runtime.spawn(struct {
            fn run(
                async_subprocess: *AsyncSubprocess, 
                args: []const []const u8, 
                timeout: ?u64,
                cb: *const fn(result: ExecResult) void
            ) !void {
                const result = try async_subprocess.exec(args, timeout);
                cb(result);
            }
        }.run, .{ self, argv, timeout_ms, callback });
    }
};