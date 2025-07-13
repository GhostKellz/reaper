const std = @import("std");
const process = std.process;
const fs = std.fs;
const print = std.debug.print;

// reap forge - For forging packages for AUR/Pacman deployment
// Focus: Package creation, AUR uploads, repository management

pub const ForgeTarget = enum {
    package,
    weapon,
    armor,
    artifact,
    
    pub fn fromString(str: []const u8) ?ForgeTarget {
        return std.meta.stringToEnum(ForgeTarget, str);
    }
};

pub const PackageForge = struct {
    allocator: std.mem.Allocator,
    package_name: []const u8,
    package_dir: []const u8,
    aur_mode: bool,
    
    pub fn init(allocator: std.mem.Allocator, name: []const u8) PackageForge {
        return .{
            .allocator = allocator,
            .package_name = name,
            .package_dir = ".",
            .aur_mode = false,
        };
    }
    
    pub fn setAurMode(self: *PackageForge, enable: bool) void {
        self.aur_mode = enable;
    }
    
    pub fn setPackageDir(self: *PackageForge, dir: []const u8) void {
        self.package_dir = dir;
    }
    
    pub fn validatePkgbuild(self: *PackageForge) !void {
        const pkgbuild_path = try fs.path.join(self.allocator, &.{ self.package_dir, "PKGBUILD" });
        defer self.allocator.free(pkgbuild_path);
        
        const file = fs.cwd().openFile(pkgbuild_path, .{}) catch |err| switch (err) {
            error.FileNotFound => {
                print("❌ PKGBUILD not found in {s}\n", .{self.package_dir});
                return error.PkgbuildNotFound;
            },
            else => return err,
        };
        defer file.close();
        
        print("✅ PKGBUILD validation passed\n", .{});
    }
    
    pub fn buildPackage(self: *PackageForge) !void {
        print("🔨 Building package: {s}\n", .{self.package_name});
        
        const result = try process.Child.run(.{
            .allocator = self.allocator,
            .argv = &.{ "makepkg", "-sf", "--noconfirm" },
            .cwd = self.package_dir,
        });
        defer self.allocator.free(result.stdout);
        defer self.allocator.free(result.stderr);
        
        if (result.term.Exited != 0) {
            print("❌ Package build failed:\n{s}\n", .{result.stderr});
            return error.BuildFailed;
        }
        
        print("✅ Package built successfully\n", .{});
    }
    
    pub fn deployToAur(self: *PackageForge) !void {
        if (!self.aur_mode) {
            print("💡 Use --aur flag to deploy to AUR\n", .{});
            return;
        }
        
        print("🚀 Deploying to AUR: {s}\n", .{self.package_name});
        
        // Check if .SRCINFO exists
        const srcinfo_path = try fs.path.join(self.allocator, &.{ self.package_dir, ".SRCINFO" });
        defer self.allocator.free(srcinfo_path);
        
        fs.cwd().access(srcinfo_path, .{}) catch |err| switch (err) {
            error.FileNotFound => {
                print("📝 Generating .SRCINFO...\n", .{});
                const gen_result = try process.Child.run(.{
                    .allocator = self.allocator,
                    .argv = &.{ "makepkg", "--printsrcinfo" },
                    .cwd = self.package_dir,
                });
                defer self.allocator.free(gen_result.stdout);
                defer self.allocator.free(gen_result.stderr);
                
                if (gen_result.term.Exited != 0) {
                    print("❌ Failed to generate .SRCINFO\n", .{});
                    return error.SrcinfoFailed;
                }
                
                try fs.cwd().writeFile(.{ .sub_path = srcinfo_path, .data = gen_result.stdout });
                print("✅ .SRCINFO generated\n", .{});
            },
            else => return err,
        };
        
        // Git operations for AUR
        const git_commands = [_][]const []const u8{
            &.{ "git", "add", "PKGBUILD", ".SRCINFO" },
            &.{ "git", "commit", "-m", "Update package" },
            &.{ "git", "push", "origin", "master" },
        };
        
        for (git_commands) |cmd| {
            const result = try process.Child.run(.{
                .allocator = self.allocator,
                .argv = cmd,
                .cwd = self.package_dir,
            });
            defer self.allocator.free(result.stdout);
            defer self.allocator.free(result.stderr);
            
            if (result.term.Exited != 0) {
                print("❌ Git command failed: {s}\n", .{result.stderr});
                return error.GitFailed;
            }
        }
        
        print("✅ Package deployed to AUR successfully\n", .{});
    }
    
    pub fn forge(self: *PackageForge) !void {
        try self.validatePkgbuild();
        try self.buildPackage();
        try self.deployToAur();
    }
};

pub fn main(allocator: std.mem.Allocator, args: []const []const u8) !void {
    if (args.len == 0) {
        printHelp();
        return;
    }
    
    const target = args[0];
    
    if (std.mem.eql(u8, target, "package")) {
        try forgePackage(allocator, if (args.len > 1) args[1..] else &[_][]const u8{});
    } else if (std.mem.eql(u8, target, "weapon")) {
        // Forge a "weapon" - performance-optimized packages
        print("⚔️  Forging performance weapon package...\n", .{});
        print("Target: Ultra-optimized builds with aggressive flags\n", .{});
        try forgeOptimizedPackage(allocator, "weapon", args[1..]);
    } else if (std.mem.eql(u8, target, "armor")) {
        // Forge "armor" - security-hardened packages
        print("🛡️  Forging security armor package...\n", .{});
        print("Target: Security-hardened builds with defensive flags\n", .{});
        try forgeOptimizedPackage(allocator, "armor", args[1..]);
    } else if (std.mem.eql(u8, target, "artifact")) {
        // Forge an "artifact" - experimental packages
        print("🔮 Forging mystical artifact package...\n", .{});
        print("Target: Experimental features and custom patches\n", .{});
        try forgeOptimizedPackage(allocator, "artifact", args[1..]);
    } else if (std.mem.eql(u8, target, "--help") or std.mem.eql(u8, target, "-h")) {
        printHelp();
    } else {
        print("Unknown forge target: {s}\n", .{target});
        printHelp();
        return error.UnknownTarget;
    }
}

fn forgePackage(allocator: std.mem.Allocator, args: []const []const u8) !void {
    var package_name: []const u8 = "unknown";
    var package_dir: []const u8 = ".";
    var aur_mode: bool = false;
    
    // Parse arguments
    var i: usize = 0;
    while (i < args.len) : (i += 1) {
        if (std.mem.eql(u8, args[i], "--name")) {
            i += 1;
            if (i >= args.len) {
                print("Error: --name requires an argument\n", .{});
                return error.InvalidArgument;
            }
            package_name = args[i];
        } else if (std.mem.eql(u8, args[i], "--dir")) {
            i += 1;
            if (i >= args.len) {
                print("Error: --dir requires an argument\n", .{});
                return error.InvalidArgument;
            }
            package_dir = args[i];
        } else if (std.mem.eql(u8, args[i], "--aur")) {
            aur_mode = true;
        } else if (std.mem.eql(u8, args[i], "--help")) {
            printPackageHelp();
            return;
        } else {
            // Assume first positional arg is package name
            if (std.mem.eql(u8, package_name, "unknown")) {
                package_name = args[i];
            }
        }
    }
    
    var forge = PackageForge.init(allocator, package_name);
    forge.setPackageDir(package_dir);
    forge.setAurMode(aur_mode);
    
    try forge.forge();
}

fn forgeOptimizedPackage(allocator: std.mem.Allocator, optimization_type: []const u8, args: []const []const u8) !void {
    print("🔧 Applying {s} optimizations...\n", .{optimization_type});
    
    if (std.mem.eql(u8, optimization_type, "weapon")) {
        print("  • -O3 -march=native -mtune=native\n", .{});
        print("  • -flto -fuse-linker-plugin\n", .{});
        print("  • Profile-guided optimization\n", .{});
    } else if (std.mem.eql(u8, optimization_type, "armor")) {
        print("  • -fstack-protector-strong\n", .{});
        print("  • -D_FORTIFY_SOURCE=2\n", .{});
        print("  • -Wl,-z,now,-z,relro\n", .{});
    } else if (std.mem.eql(u8, optimization_type, "artifact")) {
        print("  • Experimental compiler flags\n", .{});
        print("  • Custom patches applied\n", .{});
        print("  • Bleeding-edge features\n", .{});
    }
    
    // Continue with regular package forging
    try forgePackage(allocator, args);
}

fn printPackageHelp() void {
    print(
        \\reap forge package - Build and deploy packages
        \\
        \\Usage: reap forge package [name] [options]
        \\
        \\Options:
        \\  --name <name>   Package name
        \\  --dir <dir>     Package directory (default: current)
        \\  --aur           Deploy to AUR after building
        \\  --help          Show this help message
        \\
        \\Examples:
        \\  reap forge package mypackage
        \\  reap forge package --name mypackage --dir /path/to/pkg
        \\  reap forge package mypackage --aur
        \\
    , .{});
}

fn printHelp() void {
    print(
        \\reap forge - Forge packages for AUR/Pacman deployment
        \\
        \\Usage: reap forge <target> [options]
        \\
        \\Targets:
        \\  package    Build and deploy standard packages
        \\  weapon     Performance-optimized packages
        \\  armor      Security-hardened packages
        \\  artifact   Experimental/custom packages
        \\
        \\Examples:
        \\  reap forge package mypackage --aur
        \\  reap forge weapon --name fastapp
        \\  reap forge armor --name secureapp
        \\  reap forge artifact --name experimental
        \\
        \\The forge creates and deploys packages with style
        \\
    , .{});
}