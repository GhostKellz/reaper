const std = @import("std");
const Package = @import("package.zig").Package;
const Core = @import("core.zig").Core;

pub const DependencyResolver = struct {
    allocator: std.mem.Allocator,
    core: *Core,
    dependency_cache: std.HashMap([]const u8, DependencyInfo, std.hash_map.StringContext, std.hash_map.default_max_load_percentage),
    conflict_resolver: ConflictResolver,
    
    pub fn init(allocator: std.mem.Allocator, core: *Core) !*DependencyResolver {
        const self = try allocator.create(DependencyResolver);
        self.* = .{
            .allocator = allocator,
            .core = core,
            .dependency_cache = std.HashMap([]const u8, DependencyInfo, std.hash_map.StringContext, std.hash_map.default_max_load_percentage).init(allocator),
            .conflict_resolver = ConflictResolver.init(allocator),
        };
        return self;
    }
    
    pub fn deinit(self: *DependencyResolver) void {
        var iterator = self.dependency_cache.iterator();
        while (iterator.next()) |entry| {
            self.allocator.free(entry.key_ptr.*);
            entry.value_ptr.deinit(self.allocator);
        }
        self.dependency_cache.deinit();
        self.conflict_resolver.deinit();
        self.allocator.destroy(self);
    }
    
    pub fn resolve(self: *DependencyResolver, packages: []const []const u8) !ResolutionResult {
        std.debug.print("🧠 Analyzing dependencies for {} packages...\n", .{packages.len});
        
        var result = ResolutionResult.init(self.allocator);
        var queue = std.ArrayList([]const u8).init(self.allocator);
        defer queue.deinit();
        
        var visited = std.HashMap([]const u8, void, std.hash_map.StringContext, std.hash_map.default_max_load_percentage).init(self.allocator);
        defer {
            var iter = visited.iterator();
            while (iter.next()) |entry| {
                self.allocator.free(entry.key_ptr.*);
            }
            visited.deinit();
        }
        
        // Add initial packages to queue
        for (packages) |pkg_name| {
            try queue.append(try self.allocator.dupe(u8, pkg_name));
        }
        
        while (queue.items.len > 0) {
            const current_pkg = queue.pop();
            defer self.allocator.free(current_pkg);
            
            // Skip if already processed
            if (visited.contains(current_pkg)) continue;
            try visited.put(try self.allocator.dupe(u8, current_pkg), {});
            
            // Get package info
            const pkg = try self.core.getInfo(current_pkg) orelse {
                try result.addError(try std.fmt.allocPrint(self.allocator, "Package not found: {s}", .{current_pkg}));
                continue;
            };
            
            try result.addPackage(pkg);
            
            // Get dependency info (cached or fetch)
            const dep_info = try self.getDependencyInfo(current_pkg);
            
            // Process dependencies
            for (dep_info.depends) |dep| {
                if (!visited.contains(dep.name)) {
                    try queue.append(try self.allocator.dupe(u8, dep.name));
                }
            }
            
            // Check for conflicts
            try self.checkConflicts(&result, pkg, dep_info);
        }
        
        // Sort packages by dependency order
        try self.sortByDependencyOrder(&result);
        
        // Generate installation plan
        try self.generateInstallationPlan(&result);
        
        std.debug.print("✅ Dependency resolution complete: {} packages, {} conflicts\n", .{ result.packages.items.len, result.conflicts.items.len });
        
        return result;
    }
    
    fn getDependencyInfo(self: *DependencyResolver, package_name: []const u8) !DependencyInfo {
        if (self.dependency_cache.get(package_name)) |info| {
            return info;
        }
        
        // Fetch dependency info from package databases
        const info = try self.fetchDependencyInfo(package_name);
        try self.dependency_cache.put(try self.allocator.dupe(u8, package_name), info);
        
        return info;
    }
    
    fn fetchDependencyInfo(self: *DependencyResolver, package_name: []const u8) !DependencyInfo {
        _ = self;
        // This would normally query pacman/AUR databases
        // For now, return a simplified dependency info
        
        var info = DependencyInfo.init(self.allocator);
        
        // Common dependency patterns (simplified for prototype)
        if (std.mem.eql(u8, package_name, "yay")) {
            try info.addDependency("git", null);
            try info.addDependency("base-devel", null);
            try info.addDependency("go", null);
        } else if (std.mem.eql(u8, package_name, "firefox")) {
            try info.addDependency("gtk3", ">=3.14.0");
            try info.addDependency("libxtst", null);
            try info.addDependency("dbus-glib", null);
        }
        
        return info;
    }
    
    fn checkConflicts(self: *DependencyResolver, result: *ResolutionResult, pkg: Package, dep_info: DependencyInfo) !void {
        // Check for package conflicts
        for (dep_info.conflicts) |conflict| {
            for (result.packages.items) |existing_pkg| {
                if (std.mem.eql(u8, existing_pkg.name, conflict.name)) {
                    try result.addConflict(Conflict{
                        .package_a = pkg.name,
                        .package_b = existing_pkg.name,
                        .conflict_type = .package_conflict,
                        .description = try std.fmt.allocPrint(self.allocator, "{s} conflicts with {s}", .{ pkg.name, existing_pkg.name }),
                        .suggested_action = .remove_conflicting,
                    });
                }
            }
        }
        
        // Check for version conflicts
        for (dep_info.depends) |dep| {
            if (dep.version_requirement) |req| {
                if (try self.core.getInfo(dep.name)) |dep_pkg| {
                    if (!self.satisfiesVersion(dep_pkg.version, req)) {
                        try result.addConflict(Conflict{
                            .package_a = pkg.name,
                            .package_b = dep_pkg.name,
                            .conflict_type = .version_conflict,
                            .description = try std.fmt.allocPrint(self.allocator, "{s} requires {s} {s}, but {s} is installed", .{ pkg.name, dep.name, req, dep_pkg.version }),
                            .suggested_action = .upgrade_dependency,
                        });
                    }
                }
            }
        }
        
        // Check for file conflicts (simplified)
        try self.checkFileConflicts(result, pkg);
    }
    
    fn checkFileConflicts(self: *DependencyResolver, result: *ResolutionResult, pkg: Package) !void {
        _ = self;
        _ = result;
        _ = pkg;
        // This would check for overlapping files between packages
        // Implementation would require file listing capabilities
    }
    
    fn satisfiesVersion(self: *DependencyResolver, installed_version: []const u8, requirement: []const u8) bool {
        _ = self;
        // Simplified version comparison
        // Real implementation would use proper version parsing
        return std.mem.indexOf(u8, requirement, installed_version) != null;
    }
    
    fn sortByDependencyOrder(self: *DependencyResolver, result: *ResolutionResult) !void {
        // Topological sort of packages by dependencies
        // For now, just reverse the order (simple heuristic)
        _ = self;
        std.mem.reverse(Package, result.packages.items);
    }
    
    fn generateInstallationPlan(self: *DependencyResolver, result: *ResolutionResult) !void {
        std.debug.print("\n📋 Installation Plan:\n", .{});
        
        var repo_packages = std.ArrayList(Package).init(self.allocator);
        var aur_packages = std.ArrayList(Package).init(self.allocator);
        defer repo_packages.deinit();
        defer aur_packages.deinit();
        
        // Separate by source
        for (result.packages.items) |pkg| {
            if (pkg.package_type == .aur) {
                try aur_packages.append(pkg);
            } else {
                try repo_packages.append(pkg);
            }
        }
        
        // Generate plan
        if (repo_packages.items.len > 0) {
            std.debug.print("   📦 Repository packages: {}\n", .{repo_packages.items.len});
            for (repo_packages.items) |pkg| {
                std.debug.print("      • {s} {s}\n", .{ pkg.name, pkg.version });
            }
        }
        
        if (aur_packages.items.len > 0) {
            std.debug.print("   🏗️  AUR packages: {}\n", .{aur_packages.items.len});
            for (aur_packages.items) |pkg| {
                std.debug.print("      • {s} {s} [Trust: {d:.1}]\n", .{ pkg.name, pkg.version, pkg.trust_score });
            }
        }
        
        if (result.conflicts.items.len > 0) {
            std.debug.print("   ⚠️  Conflicts detected: {}\n", .{result.conflicts.items.len});
            for (result.conflicts.items) |conflict| {
                std.debug.print("      • {s}\n", .{conflict.description});
            }
        }
        
        // Calculate total download size and time estimate
        const total_size = self.calculateTotalSize(result.packages.items);
        const estimated_time = self.estimateInstallTime(result.packages.items);
        
        std.debug.print("\n   📊 Summary:\n", .{});
        std.debug.print("      Total Download: {s}\n", .{formatSize(total_size)});
        std.debug.print("      Estimated Time: {s}\n", .{formatTime(estimated_time)});
    }
    
    fn calculateTotalSize(self: *DependencyResolver, packages: []Package) u64 {
        _ = self;
        var total: u64 = 0;
        for (packages) |pkg| {
            // Estimate package size (would be fetched from package info)
            total += if (pkg.package_type == .aur) 50 * 1024 * 1024 else 20 * 1024 * 1024; // 50MB for AUR, 20MB for repo
        }
        return total;
    }
    
    fn estimateInstallTime(self: *DependencyResolver, packages: []Package) u32 {
        _ = self;
        var total_seconds: u32 = 0;
        for (packages) |pkg| {
            // Estimate install time based on package type
            total_seconds += if (pkg.package_type == .aur) 120 else 30; // 2 minutes for AUR, 30 seconds for repo
        }
        return total_seconds;
    }
};

const DependencyInfo = struct {
    depends: std.ArrayList(Dependency),
    conflicts: std.ArrayList(Dependency),
    provides: std.ArrayList([]const u8),
    optional_depends: std.ArrayList(Dependency),
    
    const Dependency = struct {
        name: []const u8,
        version_requirement: ?[]const u8,
    };
    
    pub fn init(allocator: std.mem.Allocator) DependencyInfo {
        return .{
            .depends = std.ArrayList(Dependency).init(allocator),
            .conflicts = std.ArrayList(Dependency).init(allocator),
            .provides = std.ArrayList([]const u8).init(allocator),
            .optional_depends = std.ArrayList(Dependency).init(allocator),
        };
    }
    
    pub fn deinit(self: *DependencyInfo, allocator: std.mem.Allocator) void {
        for (self.depends.items) |dep| {
            allocator.free(dep.name);
            if (dep.version_requirement) |ver| allocator.free(ver);
        }
        self.depends.deinit();
        
        for (self.conflicts.items) |dep| {
            allocator.free(dep.name);
            if (dep.version_requirement) |ver| allocator.free(ver);
        }
        self.conflicts.deinit();
        
        for (self.provides.items) |provide| {
            allocator.free(provide);
        }
        self.provides.deinit();
        
        for (self.optional_depends.items) |dep| {
            allocator.free(dep.name);
            if (dep.version_requirement) |ver| allocator.free(ver);
        }
        self.optional_depends.deinit();
    }
    
    pub fn addDependency(self: *DependencyInfo, name: []const u8, version: ?[]const u8) !void {
        try self.depends.append(.{
            .name = name,
            .version_requirement = version,
        });
    }
};

pub const ResolutionResult = struct {
    packages: std.ArrayList(Package),
    conflicts: std.ArrayList(Conflict),
    errors: std.ArrayList([]const u8),
    
    pub fn init(allocator: std.mem.Allocator) ResolutionResult {
        return .{
            .packages = std.ArrayList(Package).init(allocator),
            .conflicts = std.ArrayList(Conflict).init(allocator),
            .errors = std.ArrayList([]const u8).init(allocator),
        };
    }
    
    pub fn deinit(self: *ResolutionResult, allocator: std.mem.Allocator) void {
        self.packages.deinit();
        
        for (self.conflicts.items) |conflict| {
            allocator.free(conflict.description);
        }
        self.conflicts.deinit();
        
        for (self.errors.items) |error_msg| {
            allocator.free(error_msg);
        }
        self.errors.deinit();
    }
    
    pub fn addPackage(self: *ResolutionResult, package: Package) !void {
        // Check if package already exists
        for (self.packages.items) |existing| {
            if (std.mem.eql(u8, existing.name, package.name)) {
                return; // Already added
            }
        }
        try self.packages.append(package);
    }
    
    pub fn addConflict(self: *ResolutionResult, conflict: Conflict) !void {
        try self.conflicts.append(conflict);
    }
    
    pub fn addError(self: *ResolutionResult, error_msg: []const u8) !void {
        try self.errors.append(error_msg);
    }
    
    pub fn hasConflicts(self: *ResolutionResult) bool {
        return self.conflicts.items.len > 0;
    }
    
    pub fn hasErrors(self: *ResolutionResult) bool {
        return self.errors.items.len > 0;
    }
};

const Conflict = struct {
    package_a: []const u8,
    package_b: []const u8,
    conflict_type: ConflictType,
    description: []const u8,
    suggested_action: SuggestedAction,
    
    const ConflictType = enum {
        package_conflict,
        version_conflict,
        file_conflict,
        dependency_cycle,
    };
    
    const SuggestedAction = enum {
        remove_conflicting,
        upgrade_dependency,
        use_alternative,
        manual_resolution,
    };
};

const ConflictResolver = struct {
    allocator: std.mem.Allocator,
    resolution_strategies: std.ArrayList(ResolutionStrategy),
    
    const ResolutionStrategy = struct {
        name: []const u8,
        description: []const u8,
        priority: u32,
    };
    
    pub fn init(allocator: std.mem.Allocator) ConflictResolver {
        return .{
            .allocator = allocator,
            .resolution_strategies = std.ArrayList(ResolutionStrategy).init(allocator),
        };
    }
    
    pub fn deinit(self: *ConflictResolver) void {
        for (self.resolution_strategies.items) |strategy| {
            self.allocator.free(strategy.name);
            self.allocator.free(strategy.description);
        }
        self.resolution_strategies.deinit();
    }
    
    pub fn suggestResolution(self: *ConflictResolver, conflict: Conflict) ![]const u8 {
        _ = self;
        return switch (conflict.suggested_action) {
            .remove_conflicting => try std.fmt.allocPrint(self.allocator, "Remove {s} before installing {s}", .{ conflict.package_b, conflict.package_a }),
            .upgrade_dependency => try std.fmt.allocPrint(self.allocator, "Upgrade {s} to satisfy {s}", .{ conflict.package_b, conflict.package_a }),
            .use_alternative => try std.fmt.allocPrint(self.allocator, "Consider using an alternative to {s}", .{conflict.package_a}),
            .manual_resolution => try std.fmt.allocPrint(self.allocator, "Manual resolution required for {s} and {s}", .{ conflict.package_a, conflict.package_b }),
        };
    }
};

fn formatSize(bytes: u64) []const u8 {
    if (bytes > 1024 * 1024 * 1024) {
        return std.fmt.allocPrint(std.heap.page_allocator, "{d:.1} GB", .{@as(f64, @floatFromInt(bytes)) / (1024.0 * 1024.0 * 1024.0)}) catch "? GB";
    } else if (bytes > 1024 * 1024) {
        return std.fmt.allocPrint(std.heap.page_allocator, "{d:.1} MB", .{@as(f64, @floatFromInt(bytes)) / (1024.0 * 1024.0)}) catch "? MB";
    } else {
        return std.fmt.allocPrint(std.heap.page_allocator, "{d:.1} KB", .{@as(f64, @floatFromInt(bytes)) / 1024.0}) catch "? KB";
    }
}

fn formatTime(seconds: u32) []const u8 {
    if (seconds > 3600) {
        const hours = seconds / 3600;
        const minutes = (seconds % 3600) / 60;
        return std.fmt.allocPrint(std.heap.page_allocator, "{}h {}m", .{ hours, minutes }) catch "? hours";
    } else if (seconds > 60) {
        const minutes = seconds / 60;
        const secs = seconds % 60;
        return std.fmt.allocPrint(std.heap.page_allocator, "{}m {}s", .{ minutes, secs }) catch "? minutes";
    } else {
        return std.fmt.allocPrint(std.heap.page_allocator, "{}s", .{seconds}) catch "? seconds";
    }
}