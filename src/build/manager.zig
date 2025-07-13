const std = @import("std");
const Package = @import("../core/package.zig").Package;

pub const BuildMethod = enum {
    zmake,
    makepkg,
    auto,
};

pub const BuildOptions = struct {
    method: BuildMethod = .auto,
    parallel: bool = true,
    cache_enabled: bool = true,
    sign_packages: bool = false,
    verify_build: bool = true,
    optimization_level: u8 = 2,
    clean_build: bool = false,
    skip_dependencies: bool = false,
    no_confirm: bool = false,
    use_existing_sources: bool = false,
    gpg_key: ?[]const u8 = null,
};

pub const BuildResult = struct {
    success: bool,
    package_path: ?[]const u8,
    build_time_ms: u64,
    build_method: BuildMethod,
    cache_hit: bool = false,
    artifacts: std.ArrayList([]const u8),
    error_message: ?[]const u8 = null,
    
    pub fn deinit(self: *BuildResult, allocator: std.mem.Allocator) void {
        if (self.package_path) |path| allocator.free(path);
        if (self.error_message) |msg| allocator.free(msg);
        for (self.artifacts.items) |artifact| {
            allocator.free(artifact);
        }
        self.artifacts.deinit();
    }
};

pub const BuildManager = struct {
    allocator: std.mem.Allocator,
    zmake_available: bool,
    cache_dir: []const u8,
    build_threads: u8,
    
    pub fn init(allocator: std.mem.Allocator) !*BuildManager {
        const self = try allocator.create(BuildManager);
        self.* = .{
            .allocator = allocator,
            .zmake_available = checkZmakeAvailability(allocator),
            .cache_dir = try allocator.dupe(u8, "~/.cache/reaper/builds"),
            .build_threads = @intCast(std.Thread.getCpuCount() catch 4),
        };
        
        // Ensure cache directory exists
        std.fs.makeDirAbsolute(self.cache_dir) catch |err| switch (err) {
            error.PathAlreadyExists => {},
            else => return err,
        };
        
        return self;
    }
    
    pub fn deinit(self: *BuildManager) void {
        self.allocator.free(self.cache_dir);
        self.allocator.destroy(self);
    }
    
    pub fn buildPackage(self: *BuildManager, pkg: *const Package, work_dir: []const u8, options: BuildOptions) !BuildResult {
        const start_time = std.time.milliTimestamp();
        
        var result = BuildResult{
            .success = false,
            .package_path = null,
            .build_time_ms = 0,
            .build_method = options.method,
            .artifacts = std.ArrayList([]const u8).init(self.allocator),
        };
        
        // Determine the actual build method
        const method = self.determineBuildMethod(options.method, work_dir);
        result.build_method = method;
        
        std.debug.print(":: Building {s} with {s}\\n", .{ pkg.name, @tagName(method) });
        
        // Check cache first if enabled
        if (options.cache_enabled) {
            if (try self.checkBuildCache(pkg, options)) |cached_path| {
                result.success = true;
                result.package_path = cached_path;
                result.cache_hit = true;
                result.build_time_ms = @intCast(std.time.milliTimestamp() - start_time);
                std.debug.print(":: Using cached build for {s}\\n", .{pkg.name});
                return result;
            }
        }
        
        // Perform the actual build
        switch (method) {
            .zmake => {
                self.buildWithZmake(pkg, work_dir, options, &result) catch |err| {
                    result.error_message = try std.fmt.allocPrint(self.allocator, "zmake build failed: {}", .{err});
                    
                    // Fallback to makepkg if zmake fails
                    if (self.isMakepkgAvailable()) {
                        std.debug.print(":: zmake failed, falling back to makepkg...\\n", .{});
                        result.build_method = .makepkg;
                        self.buildWithMakepkg(pkg, work_dir, options, &result) catch |makepkg_err| {
                            if (result.error_message) |prev_msg| self.allocator.free(prev_msg);
                            result.error_message = try std.fmt.allocPrint(self.allocator, "Both zmake and makepkg failed. zmake: {}, makepkg: {}", .{ err, makepkg_err });
                        };
                    }
                };
            },
            .makepkg => {
                self.buildWithMakepkg(pkg, work_dir, options, &result) catch |err| {
                    result.error_message = try std.fmt.allocPrint(self.allocator, "makepkg build failed: {}", .{err});
                };
            },
            .auto => unreachable, // Should have been resolved by determineBuildMethod
        }
        
        result.build_time_ms = @intCast(std.time.milliTimestamp() - start_time);
        
        // Cache successful builds
        if (result.success and options.cache_enabled and result.package_path != null) {
            self.cacheBuild(pkg, result.package_path.?, options) catch |err| {
                std.debug.print(":: Warning: Failed to cache build: {}\\n", .{err});
            };
        }
        
        return result;
    }
    
    fn determineBuildMethod(self: *BuildManager, requested: BuildMethod, work_dir: []const u8) BuildMethod {
        return switch (requested) {
            .zmake => if (self.zmake_available) .zmake else .makepkg,
            .makepkg => .makepkg,
            .auto => {
                // Auto-detect based on project characteristics and availability
                if (self.zmake_available) {
                    // Check if this is a Zig project or has zmk.toml
                    if (self.hasZigProject(work_dir) or self.hasZmkConfig(work_dir)) {
                        return .zmake;
                    }
                    // Use zmake for its enhanced features even for traditional PKGBUILDs
                    return .zmake;
                }
                return .makepkg;
            },
        };
    }
    
    fn buildWithZmake(self: *BuildManager, pkg: *const Package, work_dir: []const u8, options: BuildOptions, result: *BuildResult) !void {
        std.debug.print(":: Building with zmake (enhanced performance)...\\n", .{});
        
        // Build zmake command arguments
        var args = std.ArrayList([]const u8).init(self.allocator);
        defer args.deinit();
        
        try args.append("zmake");
        try args.append("build");
        
        if (options.parallel) {
            try args.append("--parallel");
            const threads_str = try std.fmt.allocPrint(self.allocator, "--threads={d}", .{self.build_threads});
            defer self.allocator.free(threads_str);
            try args.append(threads_str);
        }
        
        if (options.cache_enabled) {
            try args.append("--cache");
        }
        
        if (options.optimization_level > 0) {
            try args.append("--optimize");
            const opt_str = try std.fmt.allocPrint(self.allocator, "--opt-level={d}", .{options.optimization_level});
            defer self.allocator.free(opt_str);
            try args.append(opt_str);
        }
        
        if (options.verify_build) {
            try args.append("--verify");
        }
        
        if (options.clean_build) {
            try args.append("--clean");
        }
        
        if (options.no_confirm) {
            try args.append("--no-confirm");
        }
        
        // Execute zmake build
        var child = std.process.Child.init(args.items, self.allocator);
        child.cwd = work_dir;
        child.stdout_behavior = .Inherit;
        child.stderr_behavior = .Inherit;
        
        try child.spawn();
        const term = try child.wait();
        
        if (term != .Exited or term.Exited != 0) {
            return error.ZmakeBuildFailed;
        }
        
        // Package the build
        try self.packageWithZmake(pkg, work_dir, options, result);
    }
    
    fn packageWithZmake(self: *BuildManager, pkg: *const Package, work_dir: []const u8, options: BuildOptions, result: *BuildResult) !void {
        var args = std.ArrayList([]const u8).init(self.allocator);
        defer args.deinit();
        
        try args.append("zmake");
        try args.append("package");
        
        if (options.sign_packages and options.gpg_key != null) {
            try args.append("--sign");
            const key_arg = try std.fmt.allocPrint(self.allocator, "--key={s}", .{options.gpg_key.?});
            defer self.allocator.free(key_arg);
            try args.append(key_arg);
        }
        
        if (options.verify_build) {
            try args.append("--verify");
        }
        
        var child = std.process.Child.init(args.items, self.allocator);
        child.cwd = work_dir;
        child.stdout_behavior = .Inherit;
        child.stderr_behavior = .Inherit;
        
        try child.spawn();
        const term = try child.wait();
        
        if (term != .Exited or term.Exited != 0) {
            return error.ZmakePackageFailed;
        }
        
        // Find the generated package file
        if (try self.findPackageFile(work_dir, pkg.name)) |pkg_path| {
            result.package_path = pkg_path;
            result.success = true;
            try result.artifacts.append(try self.allocator.dupe(u8, pkg_path));
        }
    }
    
    fn buildWithMakepkg(self: *BuildManager, pkg: *const Package, work_dir: []const u8, options: BuildOptions, result: *BuildResult) !void {
        std.debug.print(":: Building with makepkg (traditional method)...\\n", .{});
        _ = pkg;
        
        var args = std.ArrayList([]const u8).init(self.allocator);
        defer args.deinit();
        
        try args.append("makepkg");
        try args.append("-s"); // Install missing dependencies
        
        if (options.no_confirm) {
            try args.append("--noconfirm");
        }
        
        if (options.clean_build) {
            try args.append("-c"); // Clean build files after successful build
        }
        
        if (!options.verify_build) {
            try args.append("--skippgpcheck");
        }
        
        if (options.use_existing_sources) {
            try args.append("-e"); // Use existing sources in src/ dir
        }
        
        // Execute makepkg
        var child = std.process.Child.init(args.items, self.allocator);
        child.cwd = work_dir;
        child.stdout_behavior = .Inherit;
        child.stderr_behavior = .Inherit;
        
        try child.spawn();
        const term = try child.wait();
        
        if (term != .Exited or term.Exited != 0) {
            return error.MakepkgBuildFailed;
        }
        
        // Find the generated package file
        if (try self.findPackageFile(work_dir, pkg.name)) |pkg_path| {
            result.package_path = pkg_path;
            result.success = true;
            try result.artifacts.append(try self.allocator.dupe(u8, pkg_path));
        }
    }
    
    fn findPackageFile(self: *BuildManager, work_dir: []const u8, pkg_name: []const u8) !?[]const u8 {
        var dir = std.fs.openDirAbsolute(work_dir, .{ .iterate = true }) catch return null;
        defer dir.close();
        
        var walker = dir.walk(self.allocator) catch return null;
        defer walker.deinit();
        
        while (walker.next() catch null) |entry| {
            if (entry.kind == .file and std.mem.endsWith(u8, entry.path, ".pkg.tar.zst")) {
                if (std.mem.indexOf(u8, entry.path, pkg_name) != null) {
                    return try std.fs.path.join(self.allocator, &.{ work_dir, entry.path });
                }
            }
        }
        
        return null;
    }
    
    fn checkBuildCache(self: *BuildManager, pkg: *const Package, options: BuildOptions) !?[]const u8 {
        _ = self;
        _ = pkg;
        _ = options;
        // TODO: Implement caching based on package version, PKGBUILD hash, and build options
        return null;
    }
    
    fn cacheBuild(self: *BuildManager, pkg: *const Package, package_path: []const u8, options: BuildOptions) !void {
        _ = self;
        _ = pkg;
        _ = package_path;
        _ = options;
        // TODO: Implement build caching
    }
    
    fn hasZigProject(self: *BuildManager, work_dir: []const u8) bool {
        _ = self;
        
        const zig_files = [_][]const u8{ "build.zig", "build.zig.zon", "src/main.zig" };
        
        for (zig_files) |file| {
            const path = std.fs.path.join(self.allocator, &.{ work_dir, file }) catch continue;
            defer self.allocator.free(path);
            
            std.fs.accessAbsolute(path, .{}) catch continue;
            return true;
        }
        
        return false;
    }
    
    fn hasZmkConfig(self: *BuildManager, work_dir: []const u8) bool {
        const config_path = std.fs.path.join(self.allocator, &.{ work_dir, "zmk.toml" }) catch return false;
        defer self.allocator.free(config_path);
        
        std.fs.accessAbsolute(config_path, .{}) catch return false;
        return true;
    }
    
    fn isMakepkgAvailable(self: *BuildManager) bool {
        _ = self;
        return checkCommandAvailability(self.allocator, "makepkg");
    }
};

fn checkZmakeAvailability(allocator: std.mem.Allocator) bool {
    return checkCommandAvailability(allocator, "zmake");
}

fn checkCommandAvailability(allocator: std.mem.Allocator, command: []const u8) bool {
    var child = std.process.Child.init(&[_][]const u8{ "which", command }, allocator);
    child.stdout_behavior = .Pipe;
    child.stderr_behavior = .Pipe;
    
    if (child.spawnAndWait()) |term| {
        return term == .Exited and term.Exited == 0;
    } else |_| {
        return false;
    }
}