const std = @import("std");

pub const PackageType = enum {
    aur,
    pacman,
    flatpak,
    tap,
    local,
};

pub const Package = struct {
    // Arena allocator for all package strings
    arena: std.heap.ArenaAllocator,
    
    name: []const u8,
    version: []const u8,
    description: []const u8,
    url: []const u8,
    license: []const u8,
    arch: []const []const u8,
    dependencies: []const []const u8,
    make_dependencies: []const []const u8,
    optional_dependencies: []const []const u8,
    provides: []const []const u8,
    conflicts: []const []const u8,
    replaces: []const []const u8,
    
    // Package metadata
    package_type: PackageType,
    maintainer: []const u8,
    votes: u32,
    popularity: f32,
    out_of_date: bool,
    
    // Trust and security
    trust_score: f32,
    gpg_key: ?[]const u8,
    checksum: []const u8,
    
    // Build information
    pkgbuild_url: ?[]const u8,
    source_urls: []const []const u8,
    
    // Backend reference
    backend: *@import("../backends/backend.zig").Backend,
    
    pub fn init(allocator: std.mem.Allocator, name: []const u8) !Package {
        var arena = std.heap.ArenaAllocator.init(allocator);
        const arena_allocator = arena.allocator();
        
        return Package{
            .arena = arena,
            .name = try arena_allocator.dupe(u8, name),
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
            .package_type = .aur,
            .maintainer = "",
            .votes = 0,
            .popularity = 0.0,
            .out_of_date = false,
            .trust_score = 0.0,
            .gpg_key = null,
            .checksum = "",
            .pkgbuild_url = null,
            .source_urls = &.{},
            .backend = undefined,
        };
    }
    
    pub fn initFromBackend(allocator: std.mem.Allocator) !Package {
        return Package{
            .arena = std.heap.ArenaAllocator.init(allocator),
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
            .package_type = .aur,
            .maintainer = "",
            .votes = 0,
            .popularity = 0.0,
            .out_of_date = false,
            .trust_score = 0.0,
            .gpg_key = null,
            .checksum = "",
            .pkgbuild_url = null,
            .source_urls = &.{},
            .backend = undefined,
        };
    }
    
    pub fn setName(self: *Package, name: []const u8) !void {
        self.name = try self.arena.allocator().dupe(u8, name);
    }
    
    pub fn setVersion(self: *Package, version: []const u8) !void {
        self.version = try self.arena.allocator().dupe(u8, version);
    }
    
    pub fn setDescription(self: *Package, description: []const u8) !void {
        self.description = try self.arena.allocator().dupe(u8, description);
    }
    
    pub fn setUrl(self: *Package, url: []const u8) !void {
        self.url = try self.arena.allocator().dupe(u8, url);
    }
    
    pub fn setLicense(self: *Package, license: []const u8) !void {
        self.license = try self.arena.allocator().dupe(u8, license);
    }
    
    pub fn setMaintainer(self: *Package, maintainer: []const u8) !void {
        self.maintainer = try self.arena.allocator().dupe(u8, maintainer);
    }
    
    pub fn setChecksum(self: *Package, checksum: []const u8) !void {
        self.checksum = try self.arena.allocator().dupe(u8, checksum);
    }
    
    pub fn setGpgKey(self: *Package, gpg_key: ?[]const u8) !void {
        if (gpg_key) |key| {
            self.gpg_key = try self.arena.allocator().dupe(u8, key);
        } else {
            self.gpg_key = null;
        }
    }
    
    pub fn setPkgbuildUrl(self: *Package, pkgbuild_url: ?[]const u8) !void {
        if (pkgbuild_url) |url| {
            self.pkgbuild_url = try self.arena.allocator().dupe(u8, url);
        } else {
            self.pkgbuild_url = null;
        }
    }
    
    pub fn deinit(self: *Package) void {
        // Free all package strings at once via arena allocator
        self.arena.deinit();
    }
    
    pub fn calculateTrustScore(self: *Package) f32 {
        var score: f32 = 5.0;
        
        // Votes impact (0-2 points)
        if (self.votes > 100) {
            score += 2.0;
        } else if (self.votes > 50) {
            score += 1.5;
        } else if (self.votes > 10) {
            score += 1.0;
        } else if (self.votes > 0) {
            score += 0.5;
        }
        
        // Popularity impact (0-2 points)
        if (self.popularity > 10.0) {
            score += 2.0;
        } else if (self.popularity > 5.0) {
            score += 1.5;
        } else if (self.popularity > 1.0) {
            score += 1.0;
        } else if (self.popularity > 0.1) {
            score += 0.5;
        }
        
        // Security factors
        if (self.gpg_key != null) {
            score += 1.0;
        }
        if (self.out_of_date) {
            score -= 2.0;
        }
        
        // Package type bonus
        switch (self.package_type) {
            .pacman => {
                score += 2.0;
            },
            .tap => {
                score += 1.0;
            },
            .aur => {},
            .flatpak => {
                score += 0.5;
            },
            .local => {
                score -= 1.0;
            },
        }
        
        // Clamp between 0 and 10
        return @max(0.0, @min(10.0, score));
    }
};