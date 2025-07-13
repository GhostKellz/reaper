const std = @import("std");

pub const Profile = enum {
    dev,
    gaming,
    minimal,
    custom,
};

pub const Config = struct {
    allocator: std.mem.Allocator,
    
    // General settings
    profile: Profile,
    parallel_downloads: u32,
    parallel_builds: u32,
    
    // Directories
    cache_dir: []const u8,
    build_dir: []const u8,
    config_dir: []const u8,
    
    // Security settings
    check_gpg: bool,
    require_signature: bool,
    verify_checksums: bool,
    
    // UI settings
    show_trust_scores: bool,
    colored_output: bool,
    progress_bars: bool,
    
    // Build settings
    makepkg_args: []const u8,
    build_flags: []const u8,
    
    pub fn init(allocator: std.mem.Allocator) !*Config {
        const self = try allocator.create(Config);
        
        // Default settings
        self.* = .{
            .allocator = allocator,
            .profile = .dev,
            .parallel_downloads = 5,
            .parallel_builds = 2,
            .cache_dir = try allocator.dupe(u8, "/tmp/reaper"),
            .build_dir = try allocator.dupe(u8, "/tmp/reaper/build"),
            .config_dir = try allocator.dupe(u8, "~/.config/reaper"),
            .check_gpg = true,
            .require_signature = false,
            .verify_checksums = true,
            .show_trust_scores = true,
            .colored_output = true,
            .progress_bars = true,
            .makepkg_args = try allocator.dupe(u8, "--noconfirm"),
            .build_flags = try allocator.dupe(u8, "-O2"),
        };
        
        // Try to load config file (simplified for prototype)
        try self.loadConfig();
        
        return self;
    }
    
    pub fn deinit(self: *Config) void {
        self.allocator.free(self.cache_dir);
        self.allocator.free(self.build_dir);
        self.allocator.free(self.config_dir);
        self.allocator.free(self.makepkg_args);
        self.allocator.free(self.build_flags);
        self.allocator.destroy(self);
    }
    
    fn loadConfig(self: *Config) !void {
        // Simplified config loading - just use defaults for prototype
        _ = self;
        // TODO: Implement TOML config loading when needed
    }
    
    pub fn getProfileConfig(self: *Config) ProfileConfig {
        return switch (self.profile) {
            .dev => ProfileConfig{
                .packages = &.{ "base-devel", "git", "vim", "tmux" },
                .repositories = &.{ "core", "extra", "aur" },
                .build_optimizations = true,
            },
            .gaming => ProfileConfig{
                .packages = &.{ "mesa", "vulkan-icd-loader", "steam" },
                .repositories = &.{ "core", "extra", "multilib", "aur" },
                .build_optimizations = false,
            },
            .minimal => ProfileConfig{
                .packages = &.{"base"},
                .repositories = &.{ "core", "extra" },
                .build_optimizations = false,
            },
            .custom => ProfileConfig{
                .packages = &.{},
                .repositories = &.{ "core", "extra", "aur" },
                .build_optimizations = true,
            },
        };
    }
};

pub const ProfileConfig = struct {
    packages: []const []const u8,
    repositories: []const []const u8,
    build_optimizations: bool,
};