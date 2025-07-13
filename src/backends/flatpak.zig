const std = @import("std");
const Backend = @import("backend.zig").Backend;
const Package = @import("../core/package.zig").Package;

pub const FlatpakBackend = struct {
    base: Backend,
    flatpak_path: []const u8,
    
    const vtable = Backend.VTable{
        .search = search,
        .hasPackage = hasPackage,
        .getPackage = getPackage,
        .download = download,
        .build = build,
        .install = install,
        .remove = remove,
        .checkUpdate = checkUpdate,
    };
    
    pub fn init(allocator: std.mem.Allocator) !*FlatpakBackend {
        var self = try allocator.create(FlatpakBackend);
        self.* = .{
            .base = undefined,
            .flatpak_path = "/usr/bin/flatpak",
        };
        
        self.base = Backend{
            .allocator = allocator,
            .backend_type = .flatpak,
            .name = "Flatpak",
            .vtable = &vtable,
        };
        
        return self;
    }
    
    pub fn deinit(self: *FlatpakBackend) void {
        self.base.allocator.destroy(self);
    }
    
    fn search(backend: *Backend, query: []const u8) ![]Package {
        const self = @fieldParentPtr(FlatpakBackend, "base", backend);
        
        const result = try std.process.Child.run(.{
            .allocator = backend.allocator,
            .argv = &.{ self.flatpak_path, "search", query },
        });
        defer backend.allocator.free(result.stdout);
        defer backend.allocator.free(result.stderr);
        
        if (result.term != .Exited or result.term.Exited != 0) {
            return &.{};
        }
        
        return self.parseSearchOutput(backend.allocator, result.stdout);
    }
    
    fn hasPackage(backend: *Backend, name: []const u8) bool {
        const self = @fieldParentPtr(FlatpakBackend, "base", backend);
        
        const result = std.process.Child.run(.{
            .allocator = backend.allocator,
            .argv = &.{ self.flatpak_path, "info", name },
        }) catch return false;
        defer backend.allocator.free(result.stdout);
        defer backend.allocator.free(result.stderr);
        
        return result.term == .Exited and result.term.Exited == 0;
    }
    
    fn getPackage(backend: *Backend, name: []const u8) !Package {
        const self = @fieldParentPtr(FlatpakBackend, "base", backend);
        
        const result = try std.process.Child.run(.{
            .allocator = backend.allocator,
            .argv = &.{ self.flatpak_path, "info", name },
        });
        defer backend.allocator.free(result.stdout);
        defer backend.allocator.free(result.stderr);
        
        if (result.term != .Exited or result.term.Exited != 0) {
            return error.PackageNotFound;
        }
        
        return self.parsePackageInfo(backend.allocator, result.stdout, backend);
    }
    
    fn download(backend: *Backend, pkg: Package) !void {
        // Flatpak handles downloading during installation
        _ = backend;
        _ = pkg;
    }
    
    fn build(backend: *Backend, pkg: Package) !void {
        // Flatpaks are pre-built sandboxed applications
        _ = backend;
        _ = pkg;
    }
    
    fn install(backend: *Backend, pkg: Package) !void {
        const self = @fieldParentPtr(FlatpakBackend, "base", backend);
        
        const result = try std.process.Child.run(.{
            .allocator = backend.allocator,
            .argv = &.{ self.flatpak_path, "install", "-y", pkg.name },
        });
        defer backend.allocator.free(result.stdout);
        defer backend.allocator.free(result.stderr);
        
        if (result.term != .Exited or result.term.Exited != 0) {
            std.debug.print("Installation failed: {s}\n", .{result.stderr});
            return error.InstallFailed;
        }
    }
    
    fn remove(backend: *Backend, pkg: Package) !void {
        const self = @fieldParentPtr(FlatpakBackend, "base", backend);
        
        const result = try std.process.Child.run(.{
            .allocator = backend.allocator,
            .argv = &.{ self.flatpak_path, "uninstall", "-y", pkg.name },
        });
        defer backend.allocator.free(result.stdout);
        defer backend.allocator.free(result.stderr);
        
        if (result.term != .Exited or result.term.Exited != 0) {
            std.debug.print("Removal failed: {s}\n", .{result.stderr});
            return error.RemoveFailed;
        }
    }
    
    fn checkUpdate(backend: *Backend, pkg: Package) !Package {
        const self = @fieldParentPtr(FlatpakBackend, "base", backend);
        
        // Check for updates
        const result = try std.process.Child.run(.{
            .allocator = backend.allocator,
            .argv = &.{ self.flatpak_path, "list", "--updates" },
        });
        defer backend.allocator.free(result.stdout);
        defer backend.allocator.free(result.stderr);
        
        if (result.term == .Exited and result.term.Exited == 0) {
            // Check if this package has an update
            if (std.mem.indexOf(u8, result.stdout, pkg.name) != null) {
                return self.getPackage(backend, pkg.name);
            }
        }
        
        return pkg;
    }
    
    fn parseSearchOutput(self: *FlatpakBackend, allocator: std.mem.Allocator, output: []const u8) ![]Package {
        _ = self;
        var packages = std.ArrayList(Package).init(allocator);
        defer packages.deinit();
        
        var lines = std.mem.tokenize(u8, output, "\n");
        
        // Skip header line
        _ = lines.next();
        
        while (lines.next()) |line| {
            var columns = std.mem.tokenize(u8, line, "\t");
            
            const name = columns.next() orelse continue;
            const description = columns.next() orelse "";
            const app_id = columns.next() orelse "";
            const version = columns.next() orelse "";
            const branch = columns.next() orelse "";
            const remotes = columns.next() orelse "";
            
            _ = branch;
            _ = remotes;
            
            var pkg = Package{
                .name = try allocator.dupe(u8, app_id),
                .version = try allocator.dupe(u8, version),
                .description = try allocator.dupe(u8, description),
                .url = "",
                .license = "",
                .arch = &.{},
                .dependencies = &.{},
                .make_dependencies = &.{},
                .optional_dependencies = &.{},
                .provides = &.{},
                .conflicts = &.{},
                .replaces = &.{},
                .package_type = .flatpak,
                .maintainer = try allocator.dupe(u8, name),
                .votes = 0,
                .popularity = 0.0,
                .out_of_date = false,
                .trust_score = 6.0, // Flatpaks are sandboxed, moderately trusted
                .gpg_key = null,
                .checksum = "",
                .pkgbuild_url = null,
                .source_urls = &.{},
                .backend = undefined,
            };
            
            try packages.append(pkg);
        }
        
        return packages.toOwnedSlice();
    }
    
    fn parsePackageInfo(self: *FlatpakBackend, allocator: std.mem.Allocator, output: []const u8, backend: *Backend) !Package {
        _ = self;
        
        var pkg = Package{
            .name = "",
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
            .package_type = .flatpak,
            .maintainer = "",
            .votes = 0,
            .popularity = 0.0,
            .out_of_date = false,
            .trust_score = 6.0,
            .gpg_key = null,
            .checksum = "",
            .pkgbuild_url = null,
            .source_urls = &.{},
            .backend = backend,
        };
        
        var lines = std.mem.tokenize(u8, output, "\n");
        while (lines.next()) |line| {
            if (std.mem.indexOf(u8, line, ":")) |colon_pos| {
                const key = std.mem.trim(u8, line[0..colon_pos], " ");
                const value = std.mem.trim(u8, line[colon_pos + 1..], " ");
                
                if (std.mem.eql(u8, key, "ID")) {
                    pkg.name = try allocator.dupe(u8, value);
                } else if (std.mem.eql(u8, key, "Version")) {
                    pkg.version = try allocator.dupe(u8, value);
                } else if (std.mem.eql(u8, key, "Description")) {
                    pkg.description = try allocator.dupe(u8, value);
                } else if (std.mem.eql(u8, key, "Origin")) {
                    pkg.maintainer = try allocator.dupe(u8, value);
                }
            }
        }
        
        return pkg;
    }
};