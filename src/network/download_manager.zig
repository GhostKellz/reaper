const std = @import("std");
const zsync = @import("zsync");
const HttpClient = @import("http_client.zig").HttpClient;

pub const DownloadManager = struct {
    allocator: std.mem.Allocator,
    http_client: *HttpClient,
    max_concurrent: u32,
    active_downloads: std.ArrayList(*DownloadTask),
    connection_pool: ConnectionPool,
    
    pub fn init(allocator: std.mem.Allocator, max_concurrent: u32) !*DownloadManager {
        const self = try allocator.create(DownloadManager);
        self.* = .{
            .allocator = allocator,
            .http_client = try HttpClient.init(allocator),
            .max_concurrent = max_concurrent,
            .active_downloads = std.ArrayList(*DownloadTask).init(allocator),
            .connection_pool = ConnectionPool.init(allocator),
        };
        return self;
    }
    
    pub fn deinit(self: *DownloadManager) void {
        for (self.active_downloads.items) |task| {
            task.deinit();
        }
        self.active_downloads.deinit();
        self.connection_pool.deinit();
        self.http_client.deinit();
        self.allocator.destroy(self);
    }
    
    pub fn downloadPackages(self: *DownloadManager, packages: []const Package) !void {
        std.debug.print("🚀 Starting parallel downloads for {} packages...\n", .{packages.len});
        
        var downloads = std.ArrayList(*DownloadTask).init(self.allocator);
        defer {
            for (downloads.items) |task| {
                task.deinit();
            }
            downloads.deinit();
        }
        
        // Create download tasks
        for (packages) |pkg| {
            const task = try DownloadTask.init(self.allocator, pkg, self.http_client);
            try downloads.append(task);
        }
        
        // Execute downloads with concurrency limit
        var completed: u32 = 0;
        var active: u32 = 0;
        var next_index: u32 = 0;
        
        while (completed < packages.len) {
            // Start new downloads up to concurrency limit
            while (active < self.max_concurrent and next_index < downloads.items.len) {
                const task = downloads.items[next_index];
                try task.start();
                active += 1;
                next_index += 1;
                
                std.debug.print("📦 Downloading: {} ({}/{})\n", .{ task.package.name, next_index, packages.len });
            }
            
            // Check for completed downloads
            var i: u32 = 0;
            while (i < self.active_downloads.items.len) {
                const task = self.active_downloads.items[i];
                if (task.isComplete()) {
                    if (task.hasError()) {
                        std.debug.print("❌ Failed: {} - {}\n", .{ task.package.name, task.getError() });
                    } else {
                        std.debug.print("✅ Completed: {} ({})\n", .{ task.package.name, task.getSpeed() });
                    }
                    
                    _ = self.active_downloads.swapRemove(i);
                    active -= 1;
                    completed += 1;
                } else {
                    i += 1;
                }
            }
            
            // Small delay to prevent busy waiting
            std.time.sleep(10 * std.time.ns_per_ms);
        }
        
        std.debug.print("🎉 All downloads completed!\n", .{});
    }
    
    pub fn downloadSingle(self: *DownloadManager, url: []const u8, output_path: []const u8) !void {
        const task = try DownloadTask.initUrl(self.allocator, url, output_path, self.http_client);
        defer task.deinit();
        
        try task.start();
        
        while (!task.isComplete()) {
            const progress = task.getProgress();
            std.debug.print("\r📥 Progress: {d:.1}% ({} KB/s)", .{ progress.percentage, progress.speed_kbps });
            std.time.sleep(100 * std.time.ns_per_ms);
        }
        
        if (task.hasError()) {
            return error.DownloadFailed;
        }
        
        std.debug.print("\n✅ Download completed!\n", .{});
    }
};

const DownloadTask = struct {
    allocator: std.mem.Allocator,
    package: Package,
    url: []const u8,
    output_path: []const u8,
    http_client: *HttpClient,
    state: State,
    progress: Progress,
    error_message: ?[]const u8,
    start_time: i64,
    
    const State = enum {
        pending,
        downloading,
        completed,
        failed,
    };
    
    const Progress = struct {
        bytes_downloaded: u64,
        total_bytes: u64,
        percentage: f32,
        speed_kbps: u32,
    };
    
    pub fn init(allocator: std.mem.Allocator, package: Package, http_client: *HttpClient) !*DownloadTask {
        const self = try allocator.create(DownloadTask);
        
        // Build download URL from package info
        const url = try std.fmt.allocPrint(allocator, "https://aur.archlinux.org{}", .{package.package_base});
        const output_path = try std.fmt.allocPrint(allocator, "/tmp/reaper/{}.tar.gz", .{package.name});
        
        self.* = .{
            .allocator = allocator,
            .package = package,
            .url = url,
            .output_path = output_path,
            .http_client = http_client,
            .state = .pending,
            .progress = .{ .bytes_downloaded = 0, .total_bytes = 0, .percentage = 0.0, .speed_kbps = 0 },
            .error_message = null,
            .start_time = 0,
        };
        
        return self;
    }
    
    pub fn initUrl(allocator: std.mem.Allocator, url: []const u8, output_path: []const u8, http_client: *HttpClient) !*DownloadTask {
        const self = try allocator.create(DownloadTask);
        self.* = .{
            .allocator = allocator,
            .package = undefined, // Not used for URL downloads
            .url = try allocator.dupe(u8, url),
            .output_path = try allocator.dupe(u8, output_path),
            .http_client = http_client,
            .state = .pending,
            .progress = .{ .bytes_downloaded = 0, .total_bytes = 0, .percentage = 0.0, .speed_kbps = 0 },
            .error_message = null,
            .start_time = 0,
        };
        return self;
    }
    
    pub fn deinit(self: *DownloadTask) void {
        self.allocator.free(self.url);
        self.allocator.free(self.output_path);
        if (self.error_message) |msg| {
            self.allocator.free(msg);
        }
        self.allocator.destroy(self);
    }
    
    pub fn start(self: *DownloadTask) !void {
        self.state = .downloading;
        self.start_time = std.time.timestamp();
        
        // Create output directory
        const dir_path = std.fs.path.dirname(self.output_path).?;
        std.fs.makeDirAbsolute(dir_path) catch {};
        
        // Start async download
        try self.performDownload();
    }
    
    fn performDownload(self: *DownloadTask) !void {
        const file = std.fs.createFileAbsolute(self.output_path, .{}) catch |err| {
            self.state = .failed;
            self.error_message = try std.fmt.allocPrint(self.allocator, "Failed to create file: {}", .{err});
            return;
        };
        defer file.close();
        
        // Use HTTP client to download with progress tracking
        self.http_client.downloadWithProgress(self.url, file, self) catch |err| {
            self.state = .failed;
            self.error_message = try std.fmt.allocPrint(self.allocator, "Download failed: {}", .{err});
            return;
        };
        
        self.state = .completed;
    }
    
    pub fn updateProgress(self: *DownloadTask, bytes_downloaded: u64, total_bytes: u64) void {
        self.progress.bytes_downloaded = bytes_downloaded;
        self.progress.total_bytes = total_bytes;
        
        if (total_bytes > 0) {
            self.progress.percentage = @as(f32, @floatFromInt(bytes_downloaded)) / @as(f32, @floatFromInt(total_bytes)) * 100.0;
        }
        
        // Calculate speed
        const elapsed = std.time.timestamp() - self.start_time;
        if (elapsed > 0) {
            self.progress.speed_kbps = @intCast(@divTrunc(bytes_downloaded, @as(u64, @intCast(elapsed)) * 1024));
        }
    }
    
    pub fn isComplete(self: *DownloadTask) bool {
        return self.state == .completed or self.state == .failed;
    }
    
    pub fn hasError(self: *DownloadTask) bool {
        return self.state == .failed;
    }
    
    pub fn getError(self: *DownloadTask) []const u8 {
        return self.error_message orelse "Unknown error";
    }
    
    pub fn getProgress(self: *DownloadTask) Progress {
        return self.progress;
    }
    
    pub fn getSpeed(self: *DownloadTask) []const u8 {
        // Return formatted speed string
        if (self.progress.speed_kbps > 1024) {
            return std.fmt.allocPrint(self.allocator, "{d:.1} MB/s", .{@as(f32, @floatFromInt(self.progress.speed_kbps)) / 1024.0}) catch "? MB/s";
        } else {
            return std.fmt.allocPrint(self.allocator, "{} KB/s", .{self.progress.speed_kbps}) catch "? KB/s";
        }
    }
};

const ConnectionPool = struct {
    allocator: std.mem.Allocator,
    connections: std.HashMap([]const u8, *Connection, std.hash_map.StringContext, std.hash_map.default_max_load_percentage),
    
    const Connection = struct {
        url: []const u8,
        last_used: i64,
        is_alive: bool,
    };
    
    pub fn init(allocator: std.mem.Allocator) ConnectionPool {
        return .{
            .allocator = allocator,
            .connections = std.HashMap([]const u8, *Connection, std.hash_map.StringContext, std.hash_map.default_max_load_percentage).init(allocator),
        };
    }
    
    pub fn deinit(self: *ConnectionPool) void {
        var iterator = self.connections.iterator();
        while (iterator.next()) |entry| {
            self.allocator.free(entry.key_ptr.*);
            self.allocator.destroy(entry.value_ptr.*);
        }
        self.connections.deinit();
    }
    
    pub fn getConnection(self: *ConnectionPool, host: []const u8) !*Connection {
        if (self.connections.get(host)) |conn| {
            conn.last_used = std.time.timestamp();
            return conn;
        }
        
        // Create new connection
        const conn = try self.allocator.create(Connection);
        conn.* = .{
            .url = try self.allocator.dupe(u8, host),
            .last_used = std.time.timestamp(),
            .is_alive = true,
        };
        
        try self.connections.put(conn.url, conn);
        return conn;
    }
    
    pub fn cleanup(self: *ConnectionPool) void {
        const now = std.time.timestamp();
        const timeout = 300; // 5 minutes
        
        var to_remove = std.ArrayList([]const u8).init(self.allocator);
        defer to_remove.deinit();
        
        var iterator = self.connections.iterator();
        while (iterator.next()) |entry| {
            if (now - entry.value_ptr.*.last_used > timeout) {
                try to_remove.append(entry.key_ptr.*);
            }
        }
        
        for (to_remove.items) |key| {
            if (self.connections.fetchRemove(key)) |removed| {
                self.allocator.free(removed.key);
                self.allocator.destroy(removed.value);
            }
        }
    }
};

const Package = @import("../core/package.zig").Package;