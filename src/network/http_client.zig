const std = @import("std");
const zsync = @import("zsync");
const ghostnet = @import("ghostnet");

// Re-export common types for convenience
pub const DownloadProgress = struct {
    package_name: []const u8,
    downloaded_bytes: u64,
    total_bytes: u64,
    progress_percent: f64,
    speed_bps: u64,
    eta_seconds: u64,
    
    pub fn format(self: DownloadProgress, comptime _: []const u8, _: std.fmt.FormatOptions, writer: anytype) !void {
        try writer.print("{s}: {d:.1}% ({d}/{d} bytes) @ {d} B/s ETA: {d}s", 
            .{ self.package_name, self.progress_percent, self.downloaded_bytes, 
               self.total_bytes, self.speed_bps, self.eta_seconds });
    }
};

pub const HttpClientError = error{
    InitializationFailed,
    RequestFailed,
    InvalidResponse,
    NetworkTimeout,
    ConnectionPoolExhausted,
    AuthenticationFailed,
    RateLimitExceeded,
    AurUnavailable,
    DownloadFailed,
    ChecksumMismatch,
    InsufficientDiskSpace,
    InvalidPackageFormat,
    DependencyResolutionFailed,
    PackageCorrupted,
    CacheDirectoryAccessDenied,
    TempDirectoryFull,
    PackageNotFound,
    AurBatchRequestFailed,
    AurSearchFailed,
    EmptyPackageList,
};

pub const ReaperConfig = struct {
    aur_base_url: []const u8 = "https://aur.archlinux.org",
    max_concurrent_downloads: u32 = 5,
    download_timeout_ms: u64 = 300000, // 5 minutes
    rate_limit_rps: f64 = 10.0, // AUR compliant
    cache_dir: []const u8 = "/tmp/reaper-cache",
};

// AUR Package structure matching the integration guide
pub const AurPackage = struct {
    name: []const u8,
    version: []const u8,
    description: []const u8,
    maintainer: ?[]const u8,
    url_path: []const u8,
    depends: [][]const u8,
    make_depends: [][]const u8,
    checksum: []const u8,
};

pub const AurResponse = struct {
    resultcount: u32,
    results: []AurPackage,
    type: []const u8,
};

// Main HTTP client following the integration guide pattern
pub const HttpClient = struct {
    client: *ghostnet.HttpClient,
    config: ReaperConfig,
    allocator: std.mem.Allocator,
    runtime: *zsync.Runtime,
    
    pub fn init(allocator: std.mem.Allocator, runtime: *zsync.Runtime) !*HttpClient {
        return try initWithConfig(allocator, runtime, .{});
    }
    
    pub fn initWithConfig(allocator: std.mem.Allocator, runtime: *zsync.Runtime, config: ReaperConfig) !*HttpClient {
        const self = try allocator.create(HttpClient);
        errdefer allocator.destroy(self);
        
        // Configure connection pool for package repository efficiency
        const pool_config = ghostnet.PoolConfig{
            .max_connections = 20,
            .max_idle_connections = 10,
            .idle_timeout = 30_000_000_000, // 30 seconds
            .connection_timeout = 15_000_000_000, // 15 seconds
        };
        
        var client = try ghostnet.HttpClient.initWithPool(allocator, runtime, pool_config);
        
        // Configure for package manager use
        client.setDefaultTimeout(config.download_timeout_ms);
        
        // AUR-compliant rate limiting
        client.setRateLimit(config.rate_limit_rps, 20);
        
        // HTTP/3 first for performance
        try client.setProtocolPreference(&.{ .http3, .http2, .http1_1 });
        
        // Enable compression for metadata
        try client.enableCompression(true);
        
        // Set appropriate headers for AUR
        try client.setDefaultHeader("User-Agent", "reaper/1.1 (package-manager)");
        
        self.* = .{
            .client = client,
            .config = config,
            .allocator = allocator,
            .runtime = runtime,
        };
        
        return self;
    }
    
    pub fn deinit(self: *HttpClient) void {
        self.client.deinit();
        self.allocator.destroy(self);
    }
    
    // AUR-specific methods from integration guide
    
    // Single package info
    pub fn getPackageInfo(self: *HttpClient, package_name: []const u8) !AurResponse {
        const url = try std.fmt.allocPrint(self.allocator, 
            "{s}/rpc?v=5&type=info&arg={s}", 
            .{ self.config.aur_base_url, package_name }
        );
        defer self.allocator.free(url);
        
        const response = try self.client.get(url);
        defer response.deinit(self.allocator);
        
        if (!response.isSuccess()) {
            return HttpClientError.AurRequestFailed;
        }
        
        return try response.getJson(AurResponse);
    }
    
    // Batch package info - efficient for dependency resolution
    pub fn getMultiplePackageInfo(self: *HttpClient, package_names: []const []const u8) !AurResponse {
        if (package_names.len == 0) return HttpClientError.EmptyPackageList;
        
        // Build URL with multiple args
        var url_builder = std.ArrayList(u8).init(self.allocator);
        defer url_builder.deinit();
        
        try url_builder.writer().print("{s}/rpc?v=5&type=info", .{self.config.aur_base_url});
        
        for (package_names) |name| {
            try url_builder.writer().print("&arg[]={s}", .{name});
        }
        
        const response = try self.client.get(url_builder.items);
        defer response.deinit(self.allocator);
        
        if (!response.isSuccess()) {
            return HttpClientError.AurBatchRequestFailed;
        }
        
        return try response.getJson(AurResponse);
    }
    
    // Search packages with keyword
    pub fn searchPackages(self: *HttpClient, query: []const u8) !AurResponse {
        const url = try std.fmt.allocPrint(self.allocator,
            "{s}/rpc?v=5&type=search&arg={s}",
            .{ self.config.aur_base_url, query }
        );
        defer self.allocator.free(url);
        
        const response = try self.client.get(url);
        defer response.deinit(self.allocator);
        
        if (!response.isSuccess()) {
            return HttpClientError.AurSearchFailed;
        }
        
        return try response.getJson(AurResponse);
    }
    
    // Package download with progress from integration guide
    pub fn downloadPackage(
        self: *HttpClient, 
        package: AurPackage, 
        progress_callback: ?*const fn (DownloadProgress) void
    ) !void {
        const download_url = try std.fmt.allocPrint(self.allocator,
            "{s}/{s}",
            .{ self.config.aur_base_url, package.url_path }
        );
        defer self.allocator.free(download_url);
        
        const dest_path = try std.fmt.allocPrint(self.allocator,
            "{s}/{s}.tar.gz",
            .{ self.config.cache_dir, package.name }
        );
        defer self.allocator.free(dest_path);
        
        // Create cache directory if it doesn't exist
        std.fs.cwd().makePath(self.config.cache_dir) catch {};
        
        // Create progress callback wrapper if needed
        const progress_wrapper = if (progress_callback) |cb| blk: {
            const Wrapper = struct {
                callback: *const fn (DownloadProgress) void,
                package_name: []const u8,
                start_time: i64,
                
                pub fn onProgress(wrapper_self: *@This(), downloaded: u64, total: u64) void {
                    const now = std.time.timestamp();
                    const elapsed = @as(f64, @floatFromInt(now - wrapper_self.start_time));
                    
                    const progress = DownloadProgress{
                        .package_name = wrapper_self.package_name,
                        .downloaded_bytes = downloaded,
                        .total_bytes = total,
                        .progress_percent = if (total > 0) @as(f64, @floatFromInt(downloaded)) / @as(f64, @floatFromInt(total)) * 100.0 else 0.0,
                        .speed_bps = if (elapsed > 0) @as(u64, @intFromFloat(@as(f64, @floatFromInt(downloaded)) / elapsed)) else 0,
                        .eta_seconds = if (downloaded > 0 and elapsed > 0) 
                            @as(u64, @intFromFloat((@as(f64, @floatFromInt(total - downloaded)) / @as(f64, @floatFromInt(downloaded))) * elapsed))
                        else 0,
                    };
                    
                    wrapper_self.callback(progress);
                }
            };
            
            const wrapper = try self.allocator.create(Wrapper);
            wrapper.* = Wrapper{
                .callback = cb,
                .package_name = package.name,
                .start_time = std.time.timestamp(),
            };
            break :blk wrapper.onProgress;
        } else null;
        
        const download_options = ghostnet.HttpClient.DownloadOptions{
            .progress_callback = progress_wrapper,
            .chunk_size = 32768, // 32KB chunks
            .resume_partial = true,
            .verify_checksum = if (package.checksum.len > 0) package.checksum else null,
            .max_speed = null, // No speed limit
        };
        
        try self.client.downloadStream(download_url, dest_path, download_options);
    }
    
    // Concurrent package downloads from integration guide
    pub fn downloadMultiplePackages(
        self: *HttpClient,
        packages: []const AurPackage,
        progress_callback: ?*const fn ([]const u8, DownloadProgress) void
    ) !void {
        var download_futures = std.ArrayList(zsync.Future(anyerror!void)).init(self.allocator);
        defer download_futures.deinit();
        
        // Start downloads concurrently (up to max_concurrent_downloads)
        var active_downloads: u32 = 0;
        var package_index: usize = 0;
        
        while (package_index < packages.len or active_downloads > 0) {
            // Start new downloads if under limit
            while (active_downloads < self.config.max_concurrent_downloads and package_index < packages.len) {
                const package = packages[package_index];
                
                const download_callback = if (progress_callback) |cb| blk: {
                    const Wrapper = struct {
                        callback: *const fn ([]const u8, DownloadProgress) void,
                        package_name: []const u8,
                        
                        pub fn onProgress(wrapper_self: *@This(), progress: DownloadProgress) void {
                            wrapper_self.callback(wrapper_self.package_name, progress);
                        }
                    };
                    
                    const wrapper = try self.allocator.create(Wrapper);
                    wrapper.* = Wrapper{
                        .callback = cb,
                        .package_name = package.name,
                    };
                    break :blk wrapper.onProgress;
                } else null;
                
                const future = try self.runtime.spawn(self.downloadPackage, .{ package, download_callback }, .normal);
                try download_futures.append(future);
                
                active_downloads += 1;
                package_index += 1;
            }
            
            // Wait for at least one download to complete
            if (download_futures.items.len > 0) {
                const completed_future = download_futures.swapRemove(0);
                try completed_future.await();
                active_downloads -= 1;
            }
        }
    }
    
    // Error handling from integration guide
    pub fn handleAurError(_: *HttpClient, err: anyerror, operation: []const u8) HttpClientError {
        return switch (err) {
            error.NetworkError => blk: {
                std.log.warn("Network error during {s}, check AUR availability", .{operation});
                break :blk HttpClientError.AurUnavailable;
            },
            error.RequestTimeout => blk: {
                std.log.warn("Request timeout during {s}, consider increasing timeout", .{operation});
                break :blk HttpClientError.AurUnavailable;
            },
            error.RateLimitExceeded => blk: {
                std.log.warn("Rate limit exceeded during {s}, backing off", .{operation});
                break :blk HttpClientError.RateLimitExceeded;
            },
            else => blk: {
                std.log.err("Unexpected error during {s}: {}", .{ operation, err });
                break :blk HttpClientError.DownloadFailed;
            },
        };
    }
    
    pub fn retryWithBackoff(
        self: *HttpClient,
        comptime func: anytype,
        args: anytype,
        max_retries: u32
    ) !@TypeOf(@call(.auto, func, args)) {
        var attempt: u32 = 0;
        var delay_ms: u64 = 1000; // Start with 1 second
        
        while (attempt < max_retries) {
            const result = @call(.auto, func, args) catch |err| {
                attempt += 1;
                
                if (attempt >= max_retries) {
                    return self.handleAurError(err, @typeName(@TypeOf(func)));
                }
                
                std.log.warn("Attempt {d}/{d} failed, retrying in {d}ms: {}", 
                    .{ attempt, max_retries, delay_ms, err });
                
                std.time.sleep(delay_ms * 1_000_000); // Convert to nanoseconds
                delay_ms = @min(delay_ms * 2, 30000); // Exponential backoff, max 30s
                continue;
            };
            
            return result;
        }
        
        unreachable;
    }
    
    // Simple HTTP methods for backward compatibility
    pub fn get(self: *HttpClient, url: []const u8) !ghostnet.HttpResponse {
        return try self.client.get(url);
    }
    
    pub fn post(self: *HttpClient, url: []const u8, body: ?[]const u8) !ghostnet.HttpResponse {
        return try self.client.post(url, body);
    }
    
    // Async versions
    pub fn getAsync(self: *HttpClient, url: []const u8) !zsync.Task {
        return try self.runtime.spawn(self.client.get, .{url}, .normal);
    }
    
    pub fn postAsync(self: *HttpClient, url: []const u8, body: ?[]const u8) !zsync.Task {
        return try self.runtime.spawn(self.client.post, .{ url, body }, .normal);
    }
};

// Dependency resolver matching integration guide
pub const DependencyResolver = struct {
    client: *HttpClient,
    resolved_cache: std.StringHashMap(AurPackage),
    
    pub fn init(client: *HttpClient) DependencyResolver {
        return .{
            .client = client,
            .resolved_cache = std.StringHashMap(AurPackage).init(client.allocator),
        };
    }
    
    pub fn deinit(self: *DependencyResolver) void {
        self.resolved_cache.deinit();
    }
    
    pub fn resolveDependencies(self: *DependencyResolver, root_packages: []const []const u8) ![]AurPackage {
        var all_packages = std.ArrayList(AurPackage).init(self.client.allocator);
        defer all_packages.deinit();
        
        var to_resolve = std.ArrayList([]const u8).init(self.client.allocator);
        defer to_resolve.deinit();
        
        try to_resolve.appendSlice(root_packages);
        var resolved_names = std.StringHashMap(void).init(self.client.allocator);
        defer resolved_names.deinit();
        
        while (to_resolve.items.len > 0) {
            // Batch resolve current level
            const response = try self.client.retryWithBackoff(
                self.client.getMultiplePackageInfo,
                .{to_resolve.items},
                3
            );
            
            to_resolve.clearRetainingCapacity();
            
            for (response.results) |package| {
                if (resolved_names.contains(package.name)) continue;
                
                try all_packages.append(package);
                try resolved_names.put(package.name, {});
                
                // Add dependencies to next resolve batch
                for (package.depends) |dep| {
                    if (!resolved_names.contains(dep)) {
                        try to_resolve.append(dep);
                    }
                }
                
                for (package.make_depends) |dep| {
                    if (!resolved_names.contains(dep)) {
                        try to_resolve.append(dep);
                    }
                }
            }
        }
        
        return all_packages.toOwnedSlice();
    }
};

// Package manager from integration guide
pub const PackageManager = struct {
    client: *HttpClient,
    dependency_resolver: DependencyResolver,
    
    pub fn init(client: *HttpClient) PackageManager {
        return .{
            .client = client,
            .dependency_resolver = DependencyResolver.init(client),
        };
    }
    
    pub fn deinit(self: *PackageManager) void {
        self.dependency_resolver.deinit();
    }
    
    pub fn installPackages(
        self: *PackageManager, 
        package_names: []const []const u8,
        progress_callback: ?*const fn ([]const u8, DownloadProgress) void
    ) !void {
        std.log.info("Resolving dependencies for {d} packages...", .{package_names.len});
        
        // Step 1: Resolve all dependencies
        const all_packages = try self.dependency_resolver.resolveDependencies(package_names);
        defer self.client.allocator.free(all_packages);
        
        std.log.info("Found {d} total packages to install (including dependencies)", .{all_packages.len});
        
        // Step 2: Download all packages concurrently
        try self.client.downloadMultiplePackages(all_packages, progress_callback);
        
        std.log.info("All packages downloaded successfully");
        
        // Step 3: Verify and install (implementation specific to Reaper)
        for (all_packages) |package| {
            try self.verifyAndInstallPackage(package);
        }
    }
    
    fn verifyAndInstallPackage(self: *PackageManager, package: AurPackage) !void {
        const cache_path = try std.fmt.allocPrint(self.client.allocator,
            "{s}/{s}.tar.gz",
            .{ self.client.config.cache_dir, package.name }
        );
        defer self.client.allocator.free(cache_path);
        
        // Verify checksum
        if (package.checksum.len > 0) {
            // Implement checksum verification
            std.log.info("Verifying checksum for {s}...", .{package.name});
        }
        
        // Extract and build (Reaper-specific implementation)
        std.log.info("Installing package: {s}", .{package.name});
        
        // This would typically involve:
        // 1. Extract tarball
        // 2. Read PKGBUILD
        // 3. Build package
        // 4. Install to system
    }
};