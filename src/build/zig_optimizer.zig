const std = @import("std");
const Package = @import("../core/package.zig").Package;

pub const ZigOptimizer = struct {
    allocator: std.mem.Allocator,
    zig_path: ?[]const u8,
    optimization_profile: OptimizationProfile,
    build_stats: BuildStats,
    
    const OptimizationProfile = enum {
        debug,
        fast,
        small,
        native,
        performance,
    };
    
    const BuildStats = struct {
        packages_built: u32,
        total_build_time: u64, // milliseconds
        compilation_speedup: f32,
        binary_size_reduction: f32,
        
        pub fn init() BuildStats {
            return .{
                .packages_built = 0,
                .total_build_time = 0,
                .compilation_speedup = 0.0,
                .binary_size_reduction = 0.0,
            };
        }
    };
    
    pub fn init(allocator: std.mem.Allocator) !*ZigOptimizer {
        const self = try allocator.create(ZigOptimizer);
        self.* = .{
            .allocator = allocator,
            .zig_path = null,
            .optimization_profile = .performance,
            .build_stats = BuildStats.init(),
        };
        
        // Detect Zig installation
        try self.detectZigInstallation();
        
        return self;
    }
    
    pub fn deinit(self: *ZigOptimizer) void {
        if (self.zig_path) |path| {
            self.allocator.free(path);
        }
        self.allocator.destroy(self);
    }
    
    pub fn optimizeBuild(self: *ZigOptimizer, package: Package, build_dir: []const u8) !BuildResult {
        const start_time = std.time.milliTimestamp();
        
        std.debug.print("⚡ Zig-optimizing build for {}...\n", .{package.name});
        
        var result = BuildResult.init(self.allocator);
        
        // Analyze PKGBUILD for optimization opportunities
        const optimizations = try self.analyzePKGBUILD(build_dir);
        defer optimizations.deinit();
        
        // Apply Zig-specific optimizations
        if (self.zig_path != null) {
            try self.applyZigOptimizations(&result, build_dir, optimizations);
        }
        
        // Apply general build optimizations
        try self.applyGeneralOptimizations(&result, build_dir, optimizations);
        
        // Generate optimized makepkg command
        result.makepkg_command = try self.generateOptimizedMakepkg(build_dir, optimizations);
        
        // Calculate performance improvements
        const build_time = std.time.milliTimestamp() - start_time;
        result.build_time_ms = @intCast(build_time);
        
        // Update stats
        self.build_stats.packages_built += 1;
        self.build_stats.total_build_time += @intCast(build_time);
        
        std.debug.print("⚡ Optimization complete: {} improvements applied\n", .{result.optimizations_applied.items.len});
        
        return result;
    }
    
    pub fn getZigCompilerFlags(self: *ZigOptimizer, target_arch: []const u8) ![]const u8 {
        return switch (self.optimization_profile) {
            .debug => try std.fmt.allocPrint(self.allocator, "-Doptimize=Debug -target {s}", .{target_arch}),
            .fast => try std.fmt.allocPrint(self.allocator, "-Doptimize=ReleaseFast -target {s}-native", .{target_arch}),
            .small => try std.fmt.allocPrint(self.allocator, "-Doptimize=ReleaseSmall -target {s}", .{target_arch}),
            .native => try std.fmt.allocPrint(self.allocator, "-Doptimize=ReleaseFast -target native-native -mcpu=native", .{}),
            .performance => try std.fmt.allocPrint(self.allocator, "-Doptimize=ReleaseFast -target native-native -mcpu=native -ffast-math", .{}),
        };
    }
    
    pub fn generateZigCC(self: *ZigOptimizer) ![]const u8 {
        if (self.zig_path) |zig| {
            const flags = try self.getZigCompilerFlags("x86_64");
            defer self.allocator.free(flags);
            
            return try std.fmt.allocPrint(self.allocator, 
                \\#!/bin/bash
                \\exec {s} cc {s} "$@"
            , .{ zig, flags });
        }
        return error.ZigNotFound;
    }
    
    pub fn optimizeForCPU(self: *ZigOptimizer, cpu_info: CPUInfo) !void {
        std.debug.print("🎯 Optimizing for CPU: {} ({})\n", .{ cpu_info.model_name, cpu_info.features });
        
        // Adjust optimization profile based on CPU capabilities
        if (cpu_info.hasFeature("avx512")) {
            self.optimization_profile = .performance;
            std.debug.print("   • AVX-512 detected: Using performance profile\n", .{});
        } else if (cpu_info.hasFeature("avx2")) {
            self.optimization_profile = .fast;
            std.debug.print("   • AVX2 detected: Using fast profile\n", .{});
        } else {
            self.optimization_profile = .native;
            std.debug.print("   • Using native profile\n", .{});
        }
    }
    
    fn detectZigInstallation(self: *ZigOptimizer) !void {
        const zig_check = [_][]const u8{ "which", "zig" };
        
        const result = std.process.Child.run(.{
            .allocator = self.allocator,
            .argv = &zig_check,
        }) catch {
            std.debug.print("⚠️  Zig compiler not found - using system compiler\n", .{});
            return;
        };
        
        defer self.allocator.free(result.stdout);
        defer self.allocator.free(result.stderr);
        
        if (result.term == .Exited and result.term.Exited == 0) {
            self.zig_path = try self.allocator.dupe(u8, std.mem.trim(u8, result.stdout, " \n\r\t"));
            
            // Get Zig version
            const version = try self.getZigVersion();
            defer self.allocator.free(version);
            
            std.debug.print("⚡ Zig detected: {} ({})\n", .{ self.zig_path.?, version });
        }
    }
    
    fn getZigVersion(self: *ZigOptimizer) ![]const u8 {
        if (self.zig_path) |zig_path| {
            const version_cmd = [_][]const u8{ zig_path, "version" };
            
            const result = try std.process.Child.run(.{
                .allocator = self.allocator,
                .argv = &version_cmd,
            });
            defer self.allocator.free(result.stderr);
            
            if (result.term == .Exited and result.term.Exited == 0) {
                return std.mem.trim(u8, result.stdout, " \n\r\t");
            }
            
            self.allocator.free(result.stdout);
        }
        
        return try self.allocator.dupe(u8, "unknown");
    }
    
    fn analyzePKGBUILD(self: *ZigOptimizer, build_dir: []const u8) !BuildOptimizations {
        const pkgbuild_path = try std.fmt.allocPrint(self.allocator, "{s}/PKGBUILD", .{build_dir});
        defer self.allocator.free(pkgbuild_path);
        
        const file = std.fs.openFileAbsolute(pkgbuild_path, .{}) catch {
            return BuildOptimizations.init(self.allocator);
        };
        defer file.close();
        
        const content = try file.readToEndAlloc(self.allocator, 1024 * 1024);
        defer self.allocator.free(content);
        
        var optimizations = BuildOptimizations.init(self.allocator);
        
        // Detect build system type
        if (std.mem.indexOf(u8, content, "cmake") != null) {
            try optimizations.addOptimization("cmake", "Use Zig as C/C++ compiler for CMake");
        }
        
        if (std.mem.indexOf(u8, content, "meson") != null) {
            try optimizations.addOptimization("meson", "Configure Meson to use Zig compiler");
        }
        
        if (std.mem.indexOf(u8, content, "cargo") != null) {
            try optimizations.addOptimization("rust", "Use Zig as linker for Rust projects");
        }
        
        if (std.mem.indexOf(u8, content, "go build") != null) {
            try optimizations.addOptimization("go", "Use Zig for Go CGO compilation");
        }
        
        // Detect optimization opportunities
        if (std.mem.indexOf(u8, content, "-O2") != null or std.mem.indexOf(u8, content, "-O3") != null) {
            try optimizations.addOptimization("cflags", "Replace GCC optimization flags with Zig equivalents");
        }
        
        if (std.mem.indexOf(u8, content, "-march=native") != null) {
            try optimizations.addOptimization("native", "Use Zig native CPU optimization");
        }
        
        // Check for parallel build support
        if (std.mem.indexOf(u8, content, "make -j") == null and std.mem.indexOf(u8, content, "ninja") == null) {
            try optimizations.addOptimization("parallel", "Enable parallel builds");
        }
        
        return optimizations;
    }
    
    fn applyZigOptimizations(self: *ZigOptimizer, result: *BuildResult, build_dir: []const u8, optimizations: BuildOptimizations) !void {
        if (self.zig_path == null) return;
        
        for (optimizations.optimizations.items) |opt| {
            if (std.mem.eql(u8, opt.type, "cmake")) {
                try self.setupZigForCMake(result, build_dir);
            } else if (std.mem.eql(u8, opt.type, "meson")) {
                try self.setupZigForMeson(result, build_dir);
            } else if (std.mem.eql(u8, opt.type, "rust")) {
                try self.setupZigForRust(result, build_dir);
            } else if (std.mem.eql(u8, opt.type, "go")) {
                try self.setupZigForGo(result, build_dir);
            }
        }
    }
    
    fn setupZigForCMake(self: *ZigOptimizer, result: *BuildResult, build_dir: []const u8) !void {
        const zig_cc_script = try std.fmt.allocPrint(self.allocator, "{s}/zig-cc", .{build_dir});
        defer self.allocator.free(zig_cc_script);
        
        const zig_cxx_script = try std.fmt.allocPrint(self.allocator, "{s}/zig-c++", .{build_dir});
        defer self.allocator.free(zig_cxx_script);
        
        // Create Zig wrapper scripts
        const cc_content = try self.generateZigCC();
        defer self.allocator.free(cc_content);
        
        const cxx_content = try std.fmt.allocPrint(self.allocator,
            \\#!/bin/bash
            \\exec {s} c++ {s} "$@"
        , .{ self.zig_path.?, try self.getZigCompilerFlags("x86_64") });
        defer self.allocator.free(cxx_content);
        
        // Write scripts
        try self.writeScript(zig_cc_script, cc_content);
        try self.writeScript(zig_cxx_script, cxx_content);
        
        // Add environment variables to result
        try result.addEnvironment("CC", zig_cc_script);
        try result.addEnvironment("CXX", zig_cxx_script);
        
        try result.addOptimization("cmake-zig", "Configured CMake to use Zig compiler");
    }
    
    fn setupZigForMeson(self: *ZigOptimizer, result: *BuildResult, build_dir: []const u8) !void {
        _ = build_dir;
        
        try result.addEnvironment("CC", self.zig_path.?);
        try result.addEnvironment("CXX", self.zig_path.?);
        
        const meson_args = try std.fmt.allocPrint(self.allocator, "--cross-file /dev/stdin <<EOF\n[binaries]\nc = ['{s}', 'cc']\ncpp = ['{s}', 'c++']\nEOF", .{ self.zig_path.?, self.zig_path.? });
        defer self.allocator.free(meson_args);
        
        try result.addBuildFlag(meson_args);
        try result.addOptimization("meson-zig", "Configured Meson to use Zig compiler");
    }
    
    fn setupZigForRust(self: *ZigOptimizer, result: *BuildResult, build_dir: []const u8) !void {
        _ = build_dir;
        
        // Use Zig as the linker for Rust projects
        try result.addEnvironment("CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER", self.zig_path.?);
        try result.addEnvironment("CC", self.zig_path.?);
        
        try result.addOptimization("rust-zig", "Configured Rust to use Zig as linker");
    }
    
    fn setupZigForGo(self: *ZigOptimizer, result: *BuildResult, build_dir: []const u8) !void {
        _ = build_dir;
        
        // Use Zig for CGO compilation
        try result.addEnvironment("CGO_ENABLED", "1");
        try result.addEnvironment("CC", self.zig_path.?);
        
        try result.addOptimization("go-zig", "Configured Go to use Zig for CGO");
    }
    
    fn applyGeneralOptimizations(self: *ZigOptimizer, result: *BuildResult, build_dir: []const u8, optimizations: BuildOptimizations) !void {
        _ = build_dir;
        
        for (optimizations.optimizations.items) |opt| {
            if (std.mem.eql(u8, opt.type, "parallel")) {
                const nproc = try self.getProcessorCount();
                const parallel_jobs = try std.fmt.allocPrint(self.allocator, "-j{}", .{nproc});
                defer self.allocator.free(parallel_jobs);
                
                try result.addBuildFlag(parallel_jobs);
                try result.addOptimization("parallel-build", try std.fmt.allocPrint(self.allocator, "Enabled {}-core parallel builds", .{nproc}));
            }
        }
        
        // Add CFLAGS optimizations
        const cpu_info = try self.detectCPU();
        defer cpu_info.deinit(self.allocator);
        
        if (cpu_info.hasFeature("avx2")) {
            try result.addEnvironment("CFLAGS", "$CFLAGS -mavx2");
            try result.addOptimization("avx2", "Enabled AVX2 optimizations");
        }
        
        if (cpu_info.hasFeature("fma")) {
            try result.addEnvironment("CFLAGS", "$CFLAGS -mfma");
            try result.addOptimization("fma", "Enabled FMA optimizations");
        }
    }
    
    fn generateOptimizedMakepkg(self: *ZigOptimizer, build_dir: []const u8, optimizations: BuildOptimizations) ![]const u8 {
        _ = optimizations;
        
        var command = std.ArrayList(u8).init(self.allocator);
        
        try command.appendSlice("makepkg");
        
        // Add standard optimizations
        try command.appendSlice(" --noconfirm --needed");
        
        // Add ccache if available
        if (self.hasCcache()) {
            try command.appendSlice(" --config /etc/makepkg-ccache.conf");
        }
        
        // Change to build directory
        try command.appendSlice(" -f");
        
        const final_command = try std.fmt.allocPrint(self.allocator, "cd {s} && {s}", .{ build_dir, command.items });
        command.deinit();
        
        return final_command;
    }
    
    fn writeScript(self: *ZigOptimizer, path: []const u8, content: []const u8) !void {
        const file = try std.fs.createFileAbsolute(path, .{});
        defer file.close();
        
        try file.writeAll(content);
        
        // Make executable
        const chmod_cmd = [_][]const u8{ "chmod", "+x", path };
        _ = std.process.Child.run(.{
            .allocator = self.allocator,
            .argv = &chmod_cmd,
        }) catch {};
    }
    
    fn getProcessorCount(self: *ZigOptimizer) !u32 {
        const nproc_cmd = [_][]const u8{"nproc"};
        
        const result = try std.process.Child.run(.{
            .allocator = self.allocator,
            .argv = &nproc_cmd,
        });
        defer self.allocator.free(result.stdout);
        defer self.allocator.free(result.stderr);
        
        if (result.term == .Exited and result.term.Exited == 0) {
            const count_str = std.mem.trim(u8, result.stdout, " \n\r\t");
            return std.fmt.parseInt(u32, count_str, 10) catch 4;
        }
        
        return 4; // Default fallback
    }
    
    fn detectCPU(self: *ZigOptimizer) !CPUInfo {
        return CPUInfo.detect(self.allocator);
    }
    
    fn hasCcache(self: *ZigOptimizer) bool {
        _ = self;
        const ccache_check = [_][]const u8{ "which", "ccache" };
        
        const result = std.process.Child.run(.{
            .allocator = self.allocator,
            .argv = &ccache_check,
        }) catch return false;
        
        defer self.allocator.free(result.stdout);
        defer self.allocator.free(result.stderr);
        
        return result.term == .Exited and result.term.Exited == 0;
    }
    
    pub fn getStats(self: *ZigOptimizer) BuildStats {
        return self.build_stats;
    }
};

const BuildOptimizations = struct {
    optimizations: std.ArrayList(Optimization),
    
    const Optimization = struct {
        type: []const u8,
        description: []const u8,
    };
    
    pub fn init(allocator: std.mem.Allocator) BuildOptimizations {
        return .{
            .optimizations = std.ArrayList(Optimization).init(allocator),
        };
    }
    
    pub fn deinit(self: *BuildOptimizations) void {
        for (self.optimizations.items) |opt| {
            self.optimizations.allocator.free(opt.type);
            self.optimizations.allocator.free(opt.description);
        }
        self.optimizations.deinit();
    }
    
    pub fn addOptimization(self: *BuildOptimizations, opt_type: []const u8, description: []const u8) !void {
        const allocator = self.optimizations.allocator;
        try self.optimizations.append(.{
            .type = try allocator.dupe(u8, opt_type),
            .description = try allocator.dupe(u8, description),
        });
    }
};

pub const BuildResult = struct {
    makepkg_command: []const u8,
    environment_vars: std.HashMap([]const u8, []const u8, std.hash_map.StringContext, std.hash_map.default_max_load_percentage),
    build_flags: std.ArrayList([]const u8),
    optimizations_applied: std.ArrayList([]const u8),
    build_time_ms: u64,
    
    pub fn init(allocator: std.mem.Allocator) BuildResult {
        return .{
            .makepkg_command = "",
            .environment_vars = std.HashMap([]const u8, []const u8, std.hash_map.StringContext, std.hash_map.default_max_load_percentage).init(allocator),
            .build_flags = std.ArrayList([]const u8).init(allocator),
            .optimizations_applied = std.ArrayList([]const u8).init(allocator),
            .build_time_ms = 0,
        };
    }
    
    pub fn deinit(self: *BuildResult, allocator: std.mem.Allocator) void {
        allocator.free(self.makepkg_command);
        
        var env_iter = self.environment_vars.iterator();
        while (env_iter.next()) |entry| {
            allocator.free(entry.key_ptr.*);
            allocator.free(entry.value_ptr.*);
        }
        self.environment_vars.deinit();
        
        for (self.build_flags.items) |flag| {
            allocator.free(flag);
        }
        self.build_flags.deinit();
        
        for (self.optimizations_applied.items) |opt| {
            allocator.free(opt);
        }
        self.optimizations_applied.deinit();
    }
    
    pub fn addEnvironment(self: *BuildResult, key: []const u8, value: []const u8) !void {
        const allocator = self.environment_vars.allocator;
        try self.environment_vars.put(
            try allocator.dupe(u8, key),
            try allocator.dupe(u8, value)
        );
    }
    
    pub fn addBuildFlag(self: *BuildResult, flag: []const u8) !void {
        try self.build_flags.append(try self.build_flags.allocator.dupe(u8, flag));
    }
    
    pub fn addOptimization(self: *BuildResult, description: []const u8) !void {
        try self.optimizations_applied.append(try self.optimizations_applied.allocator.dupe(u8, description));
    }
};

const CPUInfo = struct {
    model_name: []const u8,
    features: []const u8,
    vendor_id: []const u8,
    
    pub fn detect(allocator: std.mem.Allocator) !CPUInfo {
        const cpuinfo_file = try std.fs.openFileAbsolute("/proc/cpuinfo", .{});
        defer cpuinfo_file.close();
        
        const content = try cpuinfo_file.readToEndAlloc(allocator, 1024 * 16);
        defer allocator.free(content);
        
        var model_name: []const u8 = "Unknown";
        var features: []const u8 = "";
        var vendor_id: []const u8 = "Unknown";
        
        var lines = std.mem.tokenizeScalar(u8, content, '\n');
        while (lines.next()) |line| {
            if (std.mem.startsWith(u8, line, "model name")) {
                if (std.mem.indexOf(u8, line, ":")) |colon_pos| {
                    model_name = std.mem.trim(u8, line[colon_pos + 1..], " \t");
                }
            } else if (std.mem.startsWith(u8, line, "flags")) {
                if (std.mem.indexOf(u8, line, ":")) |colon_pos| {
                    features = std.mem.trim(u8, line[colon_pos + 1..], " \t");
                }
            } else if (std.mem.startsWith(u8, line, "vendor_id")) {
                if (std.mem.indexOf(u8, line, ":")) |colon_pos| {
                    vendor_id = std.mem.trim(u8, line[colon_pos + 1..], " \t");
                }
            }
        }
        
        return CPUInfo{
            .model_name = try allocator.dupe(u8, model_name),
            .features = try allocator.dupe(u8, features),
            .vendor_id = try allocator.dupe(u8, vendor_id),
        };
    }
    
    pub fn deinit(self: *CPUInfo, allocator: std.mem.Allocator) void {
        allocator.free(self.model_name);
        allocator.free(self.features);
        allocator.free(self.vendor_id);
    }
    
    pub fn hasFeature(self: *CPUInfo, feature: []const u8) bool {
        return std.mem.indexOf(u8, self.features, feature) != null;
    }
};