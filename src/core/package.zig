const std = @import("std");

pub const PackageType = enum {
    aur,
    pacman,
    flatpak,
    tap,
    local,
};

pub const Package = struct {
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
    
    pub fn init(allocator: std.mem.Allocator, name: []const u8) !*Package {
        const self = try allocator.create(Package);
        self.* = .{
            .name = name,
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
        return self;
    }
    
    pub fn deinit(self: *Package, allocator: std.mem.Allocator) void {
        allocator.destroy(self);
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