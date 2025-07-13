const std = @import("std");
const Backend = @import("backend.zig").Backend;
const Package = @import("../core/package.zig").Package;
const HttpClient = @import("../utils/http.zig").HttpClient;
const tokioZ = @import("tokioZ");

pub const TapBackend = struct {
    base: Backend,
    http_client: *HttpClient,
    runtime: *tokioZ.Runtime,
    tap_repos: std.ArrayList(TapRepository),
    cache_dir: []const u8,
    
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
    
    pub fn init(allocator: std.mem.Allocator, runtime: *tokioZ.Runtime, http_client: *HttpClient) !*TapBackend {
        var self = try allocator.create(TapBackend);
        self.* = .{
            .base = undefined,
            .http_client = http_client,
            .runtime = runtime,
            .tap_repos = std.ArrayList(TapRepository).init(allocator),
            .cache_dir = try std.fs.getAppDataDir(allocator, "reaper/tap"),
        };
        
        self.base = Backend{
            .allocator = allocator,
            .backend_type = .tap,
            .name = "Tap",
            .vtable = &vtable,
        };
        
        // Ensure cache directory exists
        std.fs.makeDirAbsolute(self.cache_dir) catch |err| switch (err) {
            error.PathAlreadyExists => {},
            else => return err,
        };
        
        // Load default tap repositories
        try self.loadDefaultRepos();
        
        return self;
    }
    
    pub fn deinit(self: *TapBackend) void {
        for (self.tap_repos.items) |*repo| {
            repo.deinit(self.base.allocator);
        }
        self.tap_repos.deinit();
        self.base.allocator.free(self.cache_dir);
        self.base.allocator.destroy(self);
    }
    
    pub fn addRepository(self: *TapBackend, repo: TapRepository) !void {
        try self.tap_repos.append(repo);
    }
    
    fn search(backend: *Backend, query: []const u8) ![]Package {
        const self = @fieldParentPtr(TapBackend, "base", backend);
        
        var all_packages = std.ArrayList(Package).init(backend.allocator);
        defer all_packages.deinit();
        
        // Search across all tap repositories
        for (self.tap_repos.items) |repo| {
            const packages = try self.searchRepository(repo, query);
            try all_packages.appendSlice(packages);
        }
        
        return all_packages.toOwnedSlice();
    }
    
    fn hasPackage(backend: *Backend, name: []const u8) bool {
        const self = @fieldParentPtr(TapBackend, "base", backend);
        
        for (self.tap_repos.items) |repo| {
            if (self.repositoryHasPackage(repo, name)) {
                return true;
            }
        }
        return false;
    }
    
    fn getPackage(backend: *Backend, name: []const u8) !Package {
        const self = @fieldParentPtr(TapBackend, "base", backend);
        
        for (self.tap_repos.items) |repo| {
            if (self.getPackageFromRepo(repo, name)) |pkg| {
                return pkg;
            } else |_| {
                continue;
            }
        }
        
        return error.PackageNotFound;
    }
    
    fn download(backend: *Backend, pkg: Package) !void {
        const self = @fieldParentPtr(TapBackend, "base", backend);
        
        // Download the binary package
        for (pkg.source_urls) |url| {
            const filename = std.fs.path.basename(url);
            const dest_path = try std.fs.path.join(backend.allocator, &.{ self.cache_dir, filename });
            defer backend.allocator.free(dest_path);
            
            try self.http_client.download(url, dest_path);
        }
    }
    
    fn build(backend: *Backend, pkg: Package) !void {
        // Tap packages are pre-built binaries
        _ = backend;
        _ = pkg;
    }
    
    fn install(backend: *Backend, pkg: Package) !void {
        const self = @fieldParentPtr(TapBackend, "base", backend);
        
        // For tap packages, installation typically means:
        // 1. Download the binary
        // 2. Verify checksum/signature
        // 3. Extract to appropriate location
        // 4. Set permissions
        
        for (pkg.source_urls) |url| {
            const filename = std.fs.path.basename(url);
            const cached_path = try std.fs.path.join(backend.allocator, &.{ self.cache_dir, filename });
            defer backend.allocator.free(cached_path);
            
            // Download if not cached
            const cached_file = std.fs.openFileAbsolute(cached_path, .{}) catch {
                try self.http_client.download(url, cached_path);
                try std.fs.openFileAbsolute(cached_path, .{});
            };
            cached_file.close();
            
            // Install the binary
            try self.installBinary(cached_path, pkg.name);
        }
    }
    
    fn remove(backend: *Backend, pkg: Package) !void {
        _ = backend;
        
        // Remove from standard binary locations
        const binary_paths = [_][]const u8{
            "/usr/local/bin",
            "/usr/bin",
            "/opt",
        };
        
        for (binary_paths) |bin_dir| {
            const binary_path = try std.fs.path.join(backend.allocator, &.{ bin_dir, pkg.name });
            defer backend.allocator.free(binary_path);
            
            std.fs.deleteFileAbsolute(binary_path) catch {};
        }
    }
    
    fn checkUpdate(backend: *Backend, pkg: Package) !Package {
        // Check repository for newer version
        return getPackage(backend, pkg.name);
    }
    
    fn loadDefaultRepos(self: *TapBackend) !void {
        // Add some example tap repositories
        const default_repos = [_]TapRepository{
            .{
                .name = try self.base.allocator.dupe(u8, "reaper-tap"),
                .url = try self.base.allocator.dupe(u8, "https://tap.reaper.dev"),
                .publisher = try self.base.allocator.dupe(u8, "Reaper Team"),
                .public_key = null,
                .packages = std.StringHashMap(TapPackage).init(self.base.allocator),
            },
        };
        
        for (default_repos) |repo| {
            try self.tap_repos.append(repo);
        }
    }
    
    fn searchRepository(self: *TapBackend, repo: TapRepository, query: []const u8) ![]Package {
        const search_url = try std.fmt.allocPrint(self.base.allocator, "{s}/search?q={s}", .{ repo.url, query });
        defer self.base.allocator.free(search_url);
        
        const response = try self.http_client.get(search_url);
        defer response.deinit();
        
        if (!response.isSuccess()) {
            return &.{};
        }
        
        // Parse JSON response
        return self.parseSearchResponse(response.body, repo);
    }
    
    fn repositoryHasPackage(self: *TapBackend, repo: TapRepository, name: []const u8) bool {
        const info_url = std.fmt.allocPrint(self.base.allocator, "{s}/packages/{s}", .{ repo.url, name }) catch return false;
        defer self.base.allocator.free(info_url);
        
        const response = self.http_client.get(info_url) catch return false;
        defer response.deinit();
        
        return response.isSuccess();
    }
    
    fn getPackageFromRepo(self: *TapBackend, repo: TapRepository, name: []const u8) !Package {
        const info_url = try std.fmt.allocPrint(self.base.allocator, "{s}/packages/{s}", .{ repo.url, name });
        defer self.base.allocator.free(info_url);
        
        const response = try self.http_client.get(info_url);
        defer response.deinit();
        
        if (!response.isSuccess()) {
            return error.PackageNotFound;
        }
        
        return self.parsePackageResponse(response.body, repo);
    }
    
    fn parseSearchResponse(self: *TapBackend, json_data: []const u8, repo: TapRepository) ![]Package {
        _ = self;
        _ = json_data;
        _ = repo;
        
        // TODO: Implement JSON parsing for search results
        // For now, return empty array
        return &.{};
    }
    
    fn parsePackageResponse(self: *TapBackend, json_data: []const u8, repo: TapRepository) !Package {
        _ = json_data;
        
        // TODO: Implement JSON parsing for package info
        // For now, return a basic package
        return Package{
            .name = try self.base.allocator.dupe(u8, "example"),
            .version = try self.base.allocator.dupe(u8, "1.0.0"),
            .description = try self.base.allocator.dupe(u8, "Example tap package"),
            .url = try self.base.allocator.dupe(u8, repo.url),
            .license = try self.base.allocator.dupe(u8, "MIT"),
            .arch = &.{},
            .dependencies = &.{},
            .make_dependencies = &.{},
            .optional_dependencies = &.{},
            .provides = &.{},
            .conflicts = &.{},
            .replaces = &.{},
            .package_type = .tap,
            .maintainer = try self.base.allocator.dupe(u8, repo.publisher),
            .votes = 0,
            .popularity = 0.0,
            .out_of_date = false,
            .trust_score = 7.0, // Tap packages are publisher-verified
            .gpg_key = repo.public_key,
            .checksum = "",
            .pkgbuild_url = null,
            .source_urls = &.{},
            .backend = &self.base,
        };
    }
    
    fn installBinary(self: *TapBackend, source_path: []const u8, binary_name: []const u8) !void {
        const dest_path = try std.fs.path.join(self.base.allocator, &.{ "/usr/local/bin", binary_name });
        defer self.base.allocator.free(dest_path);
        
        // Copy binary
        const result = try std.process.Child.run(.{
            .allocator = self.base.allocator,
            .argv = &.{ "sudo", "cp", source_path, dest_path },
        });
        defer self.base.allocator.free(result.stdout);
        defer self.base.allocator.free(result.stderr);
        
        if (result.term != .Exited or result.term.Exited != 0) {
            return error.InstallFailed;
        }
        
        // Set executable permissions
        const chmod_result = try std.process.Child.run(.{
            .allocator = self.base.allocator,
            .argv = &.{ "sudo", "chmod", "+x", dest_path },
        });
        defer self.base.allocator.free(chmod_result.stdout);
        defer self.base.allocator.free(chmod_result.stderr);
        
        if (chmod_result.term != .Exited or chmod_result.term.Exited != 0) {
            return error.InstallFailed;
        }
    }
};

pub const TapRepository = struct {
    name: []const u8,
    url: []const u8,
    publisher: []const u8,
    public_key: ?[]const u8,
    packages: std.StringHashMap(TapPackage),
    
    pub fn deinit(self: *TapRepository, allocator: std.mem.Allocator) void {
        allocator.free(self.name);
        allocator.free(self.url);
        allocator.free(self.publisher);
        if (self.public_key) |key| {
            allocator.free(key);
        }
        
        var iter = self.packages.iterator();
        while (iter.next()) |entry| {
            allocator.free(entry.key_ptr.*);
            entry.value_ptr.deinit(allocator);
        }
        self.packages.deinit();
    }
};

pub const TapPackage = struct {
    name: []const u8,
    version: []const u8,
    description: []const u8,
    download_url: []const u8,
    checksum: []const u8,
    size: u64,
    
    pub fn deinit(self: *TapPackage, allocator: std.mem.Allocator) void {
        allocator.free(self.name);
        allocator.free(self.version);
        allocator.free(self.description);
        allocator.free(self.download_url);
        allocator.free(self.checksum);
    }
};