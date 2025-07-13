const std = @import("std");
const process = std.process;
const fs = std.fs;
const print = std.debug.print;

pub const IsoProfile = enum {
    gaming,
    dev,
    workstation,
    minimal,
    
    pub fn getPackages(self: IsoProfile) []const []const u8 {
        return switch (self) {
            .gaming => &.{
                "steam", "gamemode", "mangohud", "lutris", "wine-staging",
                "nvidia-utils", "lib32-nvidia-utils", "vulkan-tools",
            },
            .dev => &.{
                "base-devel", "git", "neovim", "docker", "docker-compose",
                "nodejs", "npm", "python", "rust", "go", "zig",
                "wezterm", "tmux", "lazygit",
            },
            .workstation => &.{
                "firefox", "chromium", "libreoffice-fresh", "thunderbird",
                "gimp", "inkscape", "obs-studio", "kdenlive",
                "wezterm", "neovim", "git",
            },
            .minimal => &.{
                "networkmanager", "sudo", "nano", "htop",
            },
        };
    }
};

pub const IsoBuilder = struct {
    allocator: std.mem.Allocator,
    work_dir: []const u8,
    profile: IsoProfile,
    kernel_variant: []const u8,
    include_ghostnv: bool,
    additional_packages: std.ArrayList([]const u8),
    
    pub fn init(allocator: std.mem.Allocator) IsoBuilder {
        return .{
            .allocator = allocator,
            .work_dir = "/tmp/reap-iso-build",
            .profile = .minimal,
            .kernel_variant = "linux-ghost",
            .include_ghostnv = false,
            .additional_packages = std.ArrayList([]const u8).init(allocator),
        };
    }
    
    pub fn deinit(self: *IsoBuilder) void {
        self.additional_packages.deinit();
    }
    
    pub fn setProfile(self: *IsoBuilder, profile: IsoProfile) void {
        self.profile = profile;
    }
    
    pub fn setKernel(self: *IsoBuilder, kernel: []const u8) void {
        self.kernel_variant = kernel;
    }
    
    pub fn includeGhostNv(self: *IsoBuilder, include: bool) void {
        self.include_ghostnv = include;
    }
    
    pub fn addPackage(self: *IsoBuilder, package: []const u8) !void {
        try self.additional_packages.append(package);
    }
    
    pub fn prepareWorkDir(self: *IsoBuilder) !void {
        // Clean existing work directory
        std.fs.deleteTreeAbsolute(self.work_dir) catch |err| switch (err) {
            error.FileNotFound => {},
            else => return err,
        };
        
        // Create fresh work directory structure
        try fs.makeDirAbsolute(self.work_dir);
        try fs.makeDirAbsolute(try fs.path.join(self.allocator, &.{ self.work_dir, "airootfs" }));
        try fs.makeDirAbsolute(try fs.path.join(self.allocator, &.{ self.work_dir, "efiboot" }));
        try fs.makeDirAbsolute(try fs.path.join(self.allocator, &.{ self.work_dir, "syslinux" }));
    }
    
    pub fn downloadBaseIso(self: *IsoBuilder) !void {
        print("Downloading Arch Linux base ISO...\n", .{});
        
        const result = try process.Child.run(.{
            .allocator = self.allocator,
            .argv = &.{
                "curl",
                "-L",
                "-o",
                try fs.path.join(self.allocator, &.{ self.work_dir, "archlinux-base.iso" }),
                "https://geo.mirror.pkgbuild.com/iso/latest/archlinux-x86_64.iso",
            },
        });
        defer self.allocator.free(result.stdout);
        defer self.allocator.free(result.stderr);
        
        if (result.term.Exited != 0) {
            print("Failed to download base ISO: {s}\n", .{result.stderr});
            return error.DownloadFailed;
        }
    }
    
    pub fn extractBaseIso(self: *IsoBuilder) !void {
        print("Extracting base ISO...\n", .{});
        
        const iso_path = try fs.path.join(self.allocator, &.{ self.work_dir, "archlinux-base.iso" });
        const extract_dir = try fs.path.join(self.allocator, &.{ self.work_dir, "extracted" });
        
        try fs.makeDirAbsolute(extract_dir);
        
        const result = try process.Child.run(.{
            .allocator = self.allocator,
            .argv = &.{
                "bsdtar",
                "-xf",
                iso_path,
                "-C",
                extract_dir,
            },
        });
        defer self.allocator.free(result.stdout);
        defer self.allocator.free(result.stderr);
        
        if (result.term.Exited != 0) {
            print("Failed to extract ISO: {s}\n", .{result.stderr});
            return error.ExtractFailed;
        }
    }
    
    pub fn customizePackages(self: *IsoBuilder) !void {
        print("Customizing package list...\n", .{});
        
        var packages_file = std.ArrayList(u8).init(self.allocator);
        defer packages_file.deinit();
        
        // Base packages
        try packages_file.appendSlice(
            \\base
            \\base-devel
            \\linux-firmware
            \\mkinitcpio
            \\mkinitcpio-archiso
            \\syslinux
            \\efibootmgr
            \\
        );
        
        // Add kernel variant
        try packages_file.appendSlice(self.kernel_variant);
        try packages_file.append('\n');
        
        // Add profile packages
        for (self.profile.getPackages()) |pkg| {
            try packages_file.appendSlice(pkg);
            try packages_file.append('\n');
        }
        
        // Add additional packages
        for (self.additional_packages.items) |pkg| {
            try packages_file.appendSlice(pkg);
            try packages_file.append('\n');
        }
        
        // Add Ghost stack if requested
        if (self.include_ghostnv) {
            try packages_file.appendSlice("ghostnv\n");
        }
        
        // Common useful packages
        try packages_file.appendSlice(
            \\btrfs-progs
            \\networkmanager
            \\vim
            \\htop
            \\git
            \\
        );
        
        const packages_path = try fs.path.join(
            self.allocator,
            &.{ self.work_dir, "packages.x86_64" },
        );
        
        try fs.cwd().writeFile(.{ .sub_path = packages_path, .data = packages_file.items });
    }
    
    pub fn createProfileConfig(self: *IsoBuilder) !void {
        print("Creating profile configuration...\n", .{});
        
        const profile_config = try std.fmt.allocPrint(
            self.allocator,
            \\#!/usr/bin/env bash
            \\# Ghost Reaper ISO Profile Configuration
            \\
            \\iso_name="ghostarch"
            \\iso_label="GHOST_$(date +%Y%m)"
            \\iso_publisher="Ghost Reaper <https://github.com/ghostmake>"
            \\iso_application="Ghost Arch Linux Live/Rescue ISO"
            \\iso_version="$(date +%Y.%m.%d)"
            \\install_dir="arch"
            \\bootmodes=('bios.syslinux.mbr' 'bios.syslinux.eltorito' 'uefi-x64.systemd-boot.esp' 'uefi-x64.systemd-boot.eltorito')
            \\arch="x86_64"
            \\pacman_conf="pacman.conf"
            \\airootfs_image_type="squashfs"
            \\airootfs_image_tool_options=('-comp' 'xz' '-Xbcj' 'x86' '-b' '1M' '-Xdict-size' '1M')
            \\
            ,
            .{},
        );
        defer self.allocator.free(profile_config);
        
        const config_path = try fs.path.join(
            self.allocator,
            &.{ self.work_dir, "profiledef.sh" },
        );
        
        try fs.cwd().writeFile(.{ .sub_path = config_path, .data = profile_config });
    }
    
    pub fn build(self: *IsoBuilder) !void {
        print("Building ISO with profile: {s}\n", .{@tagName(self.profile)});
        
        try self.prepareWorkDir();
        
        // For now, we'll prepare the structure
        // Full implementation would require:
        // 1. Download base ISO or use archiso
        // 2. Extract and customize
        // 3. Add custom kernel and packages
        // 4. Build with mkarchiso
        
        try self.createProfileConfig();
        try self.customizePackages();
        
        print("ISO build structure prepared at: {s}\n", .{self.work_dir});
        print("To complete the build, run:\n", .{});
        print("  mkarchiso -v -w {s} -o ./out {s}\n", .{ self.work_dir, self.work_dir });
    }
};

pub fn buildIso(allocator: std.mem.Allocator, args: []const []const u8) !void {
    var builder = IsoBuilder.init(allocator);
    defer builder.deinit();
    
    // Parse arguments
    var i: usize = 0;
    while (i < args.len) : (i += 1) {
        if (std.mem.eql(u8, args[i], "--profile")) {
            i += 1;
            if (i >= args.len) {
                print("Error: --profile requires an argument\n", .{});
                return error.InvalidArgument;
            }
            
            const profile = std.meta.stringToEnum(IsoProfile, args[i]) orelse {
                print("Error: Unknown profile '{s}'\n", .{args[i]});
                print("Valid profiles: gaming, dev, workstation, minimal\n", .{});
                return error.InvalidProfile;
            };
            builder.setProfile(profile);
        } else if (std.mem.eql(u8, args[i], "--kernel")) {
            i += 1;
            if (i >= args.len) {
                print("Error: --kernel requires an argument\n", .{});
                return error.InvalidArgument;
            }
            builder.setKernel(args[i]);
        } else if (std.mem.eql(u8, args[i], "--include")) {
            i += 1;
            if (i >= args.len) {
                print("Error: --include requires an argument\n", .{});
                return error.InvalidArgument;
            }
            if (std.mem.eql(u8, args[i], "ghostnv")) {
                builder.includeGhostNv(true);
            } else {
                try builder.addPackage(args[i]);
            }
        } else if (std.mem.eql(u8, args[i], "--help")) {
            print("Usage: reap make iso [options]\n", .{});
            print("Options:\n", .{});
            print("  --profile <name>    Set ISO profile (gaming, dev, workstation, minimal)\n", .{});
            print("  --kernel <name>     Set kernel variant (default: linux-ghost)\n", .{});
            print("  --include <pkg>     Include additional package or 'ghostnv'\n", .{});
            print("  --help              Show this help message\n", .{});
            return;
        }
    }
    
    try builder.build();
}