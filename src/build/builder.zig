const std = @import("std");
const tokioZ = @import("tokioZ");
const Parser = @import("parser.zig").Parser;

pub const BuildType = enum {
    pkgbuild,
    zmk_toml,
    zig,
    cmake,
    meson,
    make,
    cargo,
    auto_detect,
};

pub const Builder = struct {
    allocator: std.mem.Allocator,
    runtime: *tokioZ.Runtime,
    parser: *Parser,
    build_dir: []const u8,
    num_jobs: u32,
    
    pub fn init(allocator: std.mem.Allocator, runtime: *tokioZ.Runtime) !*Builder {
        var self = try allocator.create(Builder);
        self.* = .{
            .allocator = allocator,
            .runtime = runtime,
            .parser = try Parser.init(allocator),
            .build_dir = try std.fs.path.resolve(allocator, &.{"build"}),
            .num_jobs = try std.Thread.getCpuCount(),
        };
        return self;
    }
    
    pub fn deinit(self: *Builder) void {
        self.parser.deinit();
        self.allocator.free(self.build_dir);
        self.allocator.destroy(self);
    }
    
    pub fn build(self: *Builder, path: []const u8, build_type: BuildType) !void {
        const actual_type = if (build_type == .auto_detect) 
            try self.detectBuildType(path) 
        else 
            build_type;
            
        switch (actual_type) {
            .pkgbuild => try self.buildPkgbuild(path),
            .zmk_toml => try self.buildZmkToml(path),
            .zig => try self.buildZig(path),
            .cmake => try self.buildCMake(path),
            .meson => try self.buildMeson(path),
            .make => try self.buildMake(path),
            .cargo => try self.buildCargo(path),
            .auto_detect => return error.UnknownBuildType,
        }
    }
    
    fn detectBuildType(self: *Builder, path: []const u8) !BuildType {
        _ = self;
        
        // Check for build files in order of preference
        const checks = .{
            .{ "zmk.toml", BuildType.zmk_toml },
            .{ "PKGBUILD", BuildType.pkgbuild },
            .{ "build.zig", BuildType.zig },
            .{ "CMakeLists.txt", BuildType.cmake },
            .{ "meson.build", BuildType.meson },
            .{ "Makefile", BuildType.make },
            .{ "Cargo.toml", BuildType.cargo },
        };
        
        var dir = try std.fs.openDirAbsolute(path, .{});
        defer dir.close();
        
        inline for (checks) |check| {
            _ = dir.statFile(check.@"0") catch continue;
            return check.@"1";
        }
        
        return error.UnknownBuildType;
    }
    
    fn buildPkgbuild(self: *Builder, path: []const u8) !void {
        const pkgbuild_path = try std.fs.path.join(self.allocator, &.{ path, "PKGBUILD" });
        defer self.allocator.free(pkgbuild_path);
        
        // Parse PKGBUILD
        const pkgbuild = try self.parser.parsePkgbuild(pkgbuild_path);
        defer pkgbuild.deinit();
        
        // Create build directory
        const build_path = try std.fs.path.join(self.allocator, &.{ self.build_dir, pkgbuild.pkgname });
        defer self.allocator.free(build_path);
        
        try std.fs.makeDirAbsolute(build_path) catch |err| switch (err) {
            error.PathAlreadyExists => {
                // Clean existing build directory
                try std.fs.deleteTreeAbsolute(build_path);
                try std.fs.makeDirAbsolute(build_path);
            },
            else => return err,
        };
        
        // Download sources in parallel
        var download_tasks = std.ArrayList(tokioZ.Task(void)).init(self.allocator);
        defer download_tasks.deinit();
        
        for (pkgbuild.source) |source| {
            const task = try self.runtime.spawn(downloadSource, .{ self, source, build_path });
            try download_tasks.append(task);
        }
        
        for (download_tasks.items) |task| {
            try task.await();
        }
        
        // Extract sources
        try self.extractSources(build_path);
        
        // Run prepare function
        if (pkgbuild.prepare_fn) |prepare| {
            try self.runBuildFunction(prepare, build_path, pkgbuild);
        }
        
        // Run build function
        if (pkgbuild.build_fn) |build_fn| {
            try self.runBuildFunction(build_fn, build_path, pkgbuild);
        }
        
        // Run package function
        try self.runBuildFunction(pkgbuild.package_fn, build_path, pkgbuild);
        
        // Create package archive
        try self.createPackage(pkgbuild, build_path);
    }
    
    fn buildZmkToml(self: *Builder, path: []const u8) !void {
        const zmk_path = try std.fs.path.join(self.allocator, &.{ path, "zmk.toml" });
        defer self.allocator.free(zmk_path);
        
        // Parse zmk.toml
        const zmk = try self.parser.parseZmkToml(zmk_path);
        defer zmk.deinit();
        
        // Similar to PKGBUILD but with modern format
        std.debug.print("Building from zmk.toml: {s}\n", .{zmk.package.name});
        
        // TODO: Implement zmk.toml build process
    }
    
    fn buildZig(self: *Builder, path: []const u8) !void {
        std.debug.print("Building Zig project at: {s}\n", .{path});
        
        const result = try std.process.Child.run(.{
            .allocator = self.allocator,
            .argv = &.{ "zig", "build", "-Doptimize=ReleaseSafe", "-j", try std.fmt.allocPrint(self.allocator, "{}", .{self.num_jobs}) },
            .cwd = path,
        });
        defer self.allocator.free(result.stdout);
        defer self.allocator.free(result.stderr);
        
        if (result.term != .Exited or result.term.Exited != 0) {
            std.debug.print("Build failed: {s}\n", .{result.stderr});
            return error.BuildFailed;
        }
    }
    
    fn buildCMake(self: *Builder, path: []const u8) !void {
        std.debug.print("Building CMake project at: {s}\n", .{path});
        
        const build_path = try std.fs.path.join(self.allocator, &.{ path, "build" });
        defer self.allocator.free(build_path);
        
        try std.fs.makeDirAbsolute(build_path) catch |err| switch (err) {
            error.PathAlreadyExists => {},
            else => return err,
        };
        
        // Configure
        var configure_result = try std.process.Child.run(.{
            .allocator = self.allocator,
            .argv = &.{ "cmake", "..", "-DCMAKE_BUILD_TYPE=Release" },
            .cwd = build_path,
        });
        defer self.allocator.free(configure_result.stdout);
        defer self.allocator.free(configure_result.stderr);
        
        if (configure_result.term != .Exited or configure_result.term.Exited != 0) {
            std.debug.print("CMake configure failed: {s}\n", .{configure_result.stderr});
            return error.ConfigureFailed;
        }
        
        // Build
        const jobs_arg = try std.fmt.allocPrint(self.allocator, "-j{}", .{self.num_jobs});
        defer self.allocator.free(jobs_arg);
        
        var build_result = try std.process.Child.run(.{
            .allocator = self.allocator,
            .argv = &.{ "cmake", "--build", ".", "--", jobs_arg },
            .cwd = build_path,
        });
        defer self.allocator.free(build_result.stdout);
        defer self.allocator.free(build_result.stderr);
        
        if (build_result.term != .Exited or build_result.term.Exited != 0) {
            std.debug.print("CMake build failed: {s}\n", .{build_result.stderr});
            return error.BuildFailed;
        }
    }
    
    fn buildMeson(self: *Builder, path: []const u8) !void {
        std.debug.print("Building Meson project at: {s}\n", .{path});
        
        const build_path = try std.fs.path.join(self.allocator, &.{ path, "builddir" });
        defer self.allocator.free(build_path);
        
        // Setup
        var setup_result = try std.process.Child.run(.{
            .allocator = self.allocator,
            .argv = &.{ "meson", "setup", "builddir", "--buildtype=release" },
            .cwd = path,
        });
        defer self.allocator.free(setup_result.stdout);
        defer self.allocator.free(setup_result.stderr);
        
        if (setup_result.term != .Exited or setup_result.term.Exited != 0) {
            std.debug.print("Meson setup failed: {s}\n", .{setup_result.stderr});
            return error.SetupFailed;
        }
        
        // Compile
        const jobs_arg = try std.fmt.allocPrint(self.allocator, "-j{}", .{self.num_jobs});
        defer self.allocator.free(jobs_arg);
        
        var compile_result = try std.process.Child.run(.{
            .allocator = self.allocator,
            .argv = &.{ "meson", "compile", "-C", "builddir", jobs_arg },
            .cwd = path,
        });
        defer self.allocator.free(compile_result.stdout);
        defer self.allocator.free(compile_result.stderr);
        
        if (compile_result.term != .Exited or compile_result.term.Exited != 0) {
            std.debug.print("Meson compile failed: {s}\n", .{compile_result.stderr});
            return error.CompileFailed;
        }
    }
    
    fn buildMake(self: *Builder, path: []const u8) !void {
        std.debug.print("Building Make project at: {s}\n", .{path});
        
        const jobs_arg = try std.fmt.allocPrint(self.allocator, "-j{}", .{self.num_jobs});
        defer self.allocator.free(jobs_arg);
        
        const result = try std.process.Child.run(.{
            .allocator = self.allocator,
            .argv = &.{ "make", jobs_arg },
            .cwd = path,
        });
        defer self.allocator.free(result.stdout);
        defer self.allocator.free(result.stderr);
        
        if (result.term != .Exited or result.term.Exited != 0) {
            std.debug.print("Make failed: {s}\n", .{result.stderr});
            return error.BuildFailed;
        }
    }
    
    fn buildCargo(self: *Builder, path: []const u8) !void {
        std.debug.print("Building Cargo project at: {s}\n", .{path});
        
        const result = try std.process.Child.run(.{
            .allocator = self.allocator,
            .argv = &.{ "cargo", "build", "--release", "--jobs", try std.fmt.allocPrint(self.allocator, "{}", .{self.num_jobs}) },
            .cwd = path,
        });
        defer self.allocator.free(result.stdout);
        defer self.allocator.free(result.stderr);
        
        if (result.term != .Exited or result.term.Exited != 0) {
            std.debug.print("Cargo build failed: {s}\n", .{result.stderr});
            return error.BuildFailed;
        }
    }
    
    fn downloadSource(self: *Builder, source: []const u8, dest_dir: []const u8) !void {
        _ = self;
        std.debug.print("Downloading: {s} to {s}\n", .{ source, dest_dir });
        // TODO: Implement actual download with tokioZ HTTP client
    }
    
    fn extractSources(self: *Builder, build_path: []const u8) !void {
        _ = self;
        _ = build_path;
        // TODO: Implement source extraction
    }
    
    fn runBuildFunction(self: *Builder, function: []const u8, build_path: []const u8, pkgbuild: anytype) !void {
        _ = self;
        _ = function;
        _ = build_path;
        _ = pkgbuild;
        // TODO: Implement build function execution
    }
    
    fn createPackage(self: *Builder, pkgbuild: anytype, build_path: []const u8) !void {
        _ = self;
        _ = pkgbuild;
        _ = build_path;
        // TODO: Implement package creation
    }
};