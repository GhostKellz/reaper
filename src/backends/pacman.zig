const std = @import("std");
const Backend = @import("backend.zig").Backend;
const Package = @import("../core/package.zig").Package;
const PackageType = @import("../core/package.zig").PackageType;
const AsyncSubprocess = @import("../async/subprocess.zig").AsyncSubprocess;

pub const PacmanBackend = struct {
    base: Backend,
    pacman_path: []const u8,
    async_subprocess: ?*AsyncSubprocess,
    
    const vtable = Backend.VTable{
        .search = search,
        .getInfo = getInfo,
        .download = download,
        .build = build,
        .install = install,
        .remove = remove,
        .update = update,
        .checkUpdate = checkUpdate,
    };
    
    pub fn setAsyncSubprocess(self: *PacmanBackend, async_subprocess: *AsyncSubprocess) void {
        self.async_subprocess = async_subprocess;
    }
    
    pub fn init(allocator: std.mem.Allocator) !*PacmanBackend {
        const self = try allocator.create(PacmanBackend);
        self.* = .{
            .base = Backend{
                .allocator = allocator,
                .backend_type = .pacman,
                .name = "Pacman",
                .vtable = &vtable,
            },
            .pacman_path = "/usr/bin/pacman",
            .async_subprocess = null,
        };
        
        return self;
    }
    
    pub fn deinit(self: *PacmanBackend) void {
        self.base.allocator.destroy(self);
    }
    
    pub fn asBackend(self: *PacmanBackend) *Backend {
        return &self.base;
    }
    
    fn search(backend: *Backend, query: []const u8) ![]Package {
        const self = @as(*PacmanBackend, @fieldParentPtr("base", backend));
        var packages = std.ArrayList(Package).init(backend.allocator);
        defer packages.deinit();
        
        const result = try std.process.Child.run(.{
            .allocator = backend.allocator,
            .argv = &.{ self.pacman_path, "-Ss", query },
        });
        defer backend.allocator.free(result.stdout);
        defer backend.allocator.free(result.stderr);
        
        if (result.term != .Exited or result.term.Exited != 0) {
            return packages.toOwnedSlice();
        }
        
        // Parse pacman output
        var lines = std.mem.tokenizeScalar(u8, result.stdout, '\n');
        while (lines.next()) |line| {
            if (std.mem.startsWith(u8, line, " ")) continue; // Skip description lines
            
            // Parse package line: repo/name version
            var parts = std.mem.tokenizeScalar(u8, line, ' ');
            const repo_name = parts.next() orelse continue;
            const version = parts.next() orelse continue;
            
            // Split repo/name
            const slash_pos = std.mem.indexOf(u8, repo_name, "/") orelse continue;
            const name = repo_name[slash_pos + 1..];
            
            // Get description from next line
            const desc = lines.next() orelse "";
            const desc_trimmed = std.mem.trim(u8, desc, " \t");
            
            const pkg = Package{
                .name = try backend.allocator.dupe(u8, name),
                .version = try backend.allocator.dupe(u8, version),
                .description = try backend.allocator.dupe(u8, desc_trimmed),
                .url = "",
                .license = "",
                .arch = &.{},
                .dependencies = &.{},
                .make_dependencies = &.{},
                .optional_dependencies = &.{},
                .provides = &.{},
                .conflicts = &.{},
                .replaces = &.{},
                .package_type = .pacman,
                .maintainer = "Arch Linux",
                .votes = 0,
                .popularity = 0.0,
                .out_of_date = false,
                .trust_score = 8.0, // High trust for official packages
                .gpg_key = null,
                .checksum = "",
                .pkgbuild_url = null,
                .source_urls = &.{},
                .backend = backend,
            };
            
            try packages.append(pkg);
        }
        
        return packages.toOwnedSlice();
    }
    
    fn getInfo(backend: *Backend, package_name: []const u8) !?Package {
        const self = @as(*PacmanBackend, @fieldParentPtr("base", backend));
        
        const result = try std.process.Child.run(.{
            .allocator = backend.allocator,
            .argv = &.{ self.pacman_path, "-Si", package_name },
        });
        defer backend.allocator.free(result.stdout);
        defer backend.allocator.free(result.stderr);
        
        if (result.term != .Exited or result.term.Exited != 0) {
            return null;
        }
        
        // Parse package info
        var pkg = Package{
            .name = try backend.allocator.dupe(u8, package_name),
            .version = "",
            .description = "",
            .url = "",
            .license = "",
            .arch = &.{},
            .dependencies = &.{},
            .make_dependencies = &.{},
            .optional_dependencies = &.{},
            .provides = &.{},
            .conflicts = &.{},
            .replaces = &.{},
            .package_type = .pacman,
            .maintainer = "Arch Linux",
            .votes = 0,
            .popularity = 0.0,
            .out_of_date = false,
            .trust_score = 8.0,
            .gpg_key = null,
            .checksum = "",
            .pkgbuild_url = null,
            .source_urls = &.{},
            .backend = backend,
        };
        
        var lines = std.mem.tokenizeScalar(u8, result.stdout, '\n');
        while (lines.next()) |line| {
            const colon_pos = std.mem.indexOf(u8, line, ":") orelse continue;
            const key = std.mem.trim(u8, line[0..colon_pos], " \t");
            const value = std.mem.trim(u8, line[colon_pos + 1..], " \t");
            
            if (std.mem.eql(u8, key, "Version")) {
                pkg.version = try backend.allocator.dupe(u8, value);
            } else if (std.mem.eql(u8, key, "Description")) {
                pkg.description = try backend.allocator.dupe(u8, value);
            } else if (std.mem.eql(u8, key, "URL")) {
                pkg.url = try backend.allocator.dupe(u8, value);
            } else if (std.mem.eql(u8, key, "Licenses")) {
                pkg.license = try backend.allocator.dupe(u8, value);
            }
        }
        
        return pkg;
    }
    
    fn install(backend: *Backend, pkg: Package) !void {
        const self = @as(*PacmanBackend, @fieldParentPtr("base", backend));
        const argv = [_][]const u8{ "sudo", self.pacman_path, "-S", "--noconfirm", pkg.name };
        
        if (self.async_subprocess) |async_proc| {
            const exit_code = try async_proc.execInheritIO(&argv, 300_000); // 5 minute timeout
            if (exit_code != 0) {
                return error.InstallFailed;
            }
        } else {
            var child = std.process.Child.init(&argv, backend.allocator);
            child.stdout_behavior = .Inherit;
            child.stderr_behavior = .Inherit;
            
            try child.spawn();
            const term = try child.wait();
            
            if (term != .Exited or term.Exited != 0) {
                return error.InstallFailed;
            }
        }
    }
    
    fn remove(backend: *Backend, pkg: Package) !void {
        const self = @as(*PacmanBackend, @fieldParentPtr("base", backend));
        const argv = [_][]const u8{ "sudo", self.pacman_path, "-R", "--noconfirm", pkg.name };
        
        if (self.async_subprocess) |async_proc| {
            const exit_code = try async_proc.execInheritIO(&argv, 300_000); // 5 minute timeout
            if (exit_code != 0) {
                return error.RemoveFailed;
            }
        } else {
            var child = std.process.Child.init(&argv, backend.allocator);
            child.stdout_behavior = .Inherit;
            child.stderr_behavior = .Inherit;
            
            try child.spawn();
            const term = try child.wait();
            
            if (term != .Exited or term.Exited != 0) {
                return error.RemoveFailed;
            }
        }
    }
    
    fn update(backend: *Backend, pkg: Package) !void {
        const self = @as(*PacmanBackend, @fieldParentPtr("base", backend));
        const argv = [_][]const u8{ "sudo", self.pacman_path, "-S", "--noconfirm", pkg.name };
        
        if (self.async_subprocess) |async_proc| {
            const exit_code = try async_proc.execInheritIO(&argv, 300_000); // 5 minute timeout
            if (exit_code != 0) {
                return error.UpdateFailed;
            }
        } else {
            var child = std.process.Child.init(&argv, backend.allocator);
            child.stdout_behavior = .Inherit;
            child.stderr_behavior = .Inherit;
            
            try child.spawn();
            const term = try child.wait();
            
            if (term != .Exited or term.Exited != 0) {
                return error.UpdateFailed;
            }
        }
    }
    
    fn checkUpdate(backend: *Backend, pkg: Package) !?[]const u8 {
        const self = @as(*PacmanBackend, @fieldParentPtr("base", backend));
        
        const result = try std.process.Child.run(.{
            .allocator = backend.allocator,
            .argv = &.{ self.pacman_path, "-Si", pkg.name },
        });
        defer backend.allocator.free(result.stdout);
        defer backend.allocator.free(result.stderr);
        
        if (result.term != .Exited or result.term.Exited != 0) {
            return null;
        }
        
        // Parse version from output
        var lines = std.mem.tokenizeScalar(u8, result.stdout, '\n');
        while (lines.next()) |line| {
            if (std.mem.startsWith(u8, line, "Version")) {
                const colon_pos = std.mem.indexOf(u8, line, ":") orelse continue;
                const version = std.mem.trim(u8, line[colon_pos + 1..], " \t");
                if (!std.mem.eql(u8, version, pkg.version)) {
                    return try backend.allocator.dupe(u8, version);
                }
                break;
            }
        }
        
        return null;
    }
    
    fn download(backend: *Backend, pkg: Package) !void {
        _ = backend;
        _ = pkg;
        // Pacman handles downloads internally
    }
    
    fn build(backend: *Backend, pkg: Package) !void {
        _ = backend;
        _ = pkg;
        // Pacman packages are pre-built
    }
    
    pub fn systemUpdate(self: *PacmanBackend, sync: bool) !void {
        const allocator = self.base.allocator;
        const timeout_ms: u64 = 600_000; // 10 minute timeout
        
        if (self.async_subprocess) |async_proc| {
            // Use async subprocess with timeout
            if (sync) {
                const sync_argv = [_][]const u8{ "sudo", self.pacman_path, "-Sy" };
                const sync_exit = try async_proc.execInheritIO(&sync_argv, timeout_ms);
                if (sync_exit != 0) {
                    return error.DatabaseSyncFailed;
                }
            }
            
            const upgrade_argv = [_][]const u8{ "sudo", self.pacman_path, "-Su", "--noconfirm" };
            const upgrade_exit = try async_proc.execInheritIO(&upgrade_argv, timeout_ms);
            if (upgrade_exit != 0) {
                return error.SystemUpgradeFailed;
            }
        } else {
            // Fallback to sync execution
            if (sync) {
                const sync_argv = [_][]const u8{ "sudo", self.pacman_path, "-Sy" };
                
                var sync_child = std.process.Child.init(&sync_argv, allocator);
                sync_child.stdout_behavior = .Inherit;
                sync_child.stderr_behavior = .Inherit;
                
                try sync_child.spawn();
                const sync_term = try sync_child.wait();
                
                if (sync_term != .Exited or sync_term.Exited != 0) {
                    return error.DatabaseSyncFailed;
                }
            }
            
            const upgrade_argv = [_][]const u8{ "sudo", self.pacman_path, "-Su", "--noconfirm" };
            
            var upgrade_child = std.process.Child.init(&upgrade_argv, allocator);
            upgrade_child.stdout_behavior = .Inherit;
            upgrade_child.stderr_behavior = .Inherit;
            
            try upgrade_child.spawn();
            const upgrade_term = try upgrade_child.wait();
            
            if (upgrade_term != .Exited or upgrade_term.Exited != 0) {
                return error.SystemUpgradeFailed;
            }
        }
    }
};