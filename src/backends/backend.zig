const std = @import("std");
const Package = @import("../core/package.zig").Package;

pub const BackendType = enum {
    aur,
    pacman,
    flatpak,
    tap,
};

pub const Backend = struct {
    allocator: std.mem.Allocator,
    backend_type: BackendType,
    name: []const u8,
    
    // Virtual table for backend operations
    vtable: *const VTable,
    
    pub const VTable = struct {
        search: *const fn (self: *Backend, query: []const u8) anyerror![]Package,
        getInfo: *const fn (self: *Backend, name: []const u8) anyerror!?Package,
        download: *const fn (self: *Backend, pkg: Package) anyerror!void,
        build: *const fn (self: *Backend, pkg: Package) anyerror!void,
        install: *const fn (self: *Backend, pkg: Package) anyerror!void,
        remove: *const fn (self: *Backend, pkg: Package) anyerror!void,
        update: *const fn (self: *Backend, pkg: Package) anyerror!void,
        checkUpdate: *const fn (self: *Backend, pkg: Package) anyerror!?[]const u8,
    };
    
    pub fn init(allocator: std.mem.Allocator, backend_type: BackendType, name: []const u8, vtable: *const VTable) !*Backend {
        const self = try allocator.create(Backend);
        self.* = .{
            .allocator = allocator,
            .backend_type = backend_type,
            .name = name,
            .vtable = vtable,
        };
        return self;
    }
    
    pub fn deinit(self: *Backend) void {
        self.allocator.destroy(self);
    }
    
    pub fn search(self: *Backend, query: []const u8) ![]Package {
        return self.vtable.search(self, query);
    }
    
    pub fn hasPackage(self: *Backend, name: []const u8) bool {
        return self.vtable.hasPackage(self, name);
    }
    
    pub fn getPackage(self: *Backend, name: []const u8) !Package {
        return self.vtable.getPackage(self, name);
    }
    
    pub fn download(self: *Backend, pkg: Package) !void {
        return self.vtable.download(self, pkg);
    }
    
    pub fn build(self: *Backend, pkg: Package) !void {
        return self.vtable.build(self, pkg);
    }
    
    pub fn install(self: *Backend, pkg: Package) !void {
        return self.vtable.install(self, pkg);
    }
    
    pub fn remove(self: *Backend, pkg: Package) !void {
        return self.vtable.remove(self, pkg);
    }
    
    pub fn checkUpdate(self: *Backend, pkg: Package) !Package {
        return self.vtable.checkUpdate(self, pkg);
    }
};