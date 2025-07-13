const std = @import("std");
const tokioZ = @import("tokioZ");
const zcrypto = @import("zcrypto");

pub const CacheError = error{
    EntryNotFound,
    CorruptedEntry,
    InsufficientSpace,
    InvalidChecksum,
};

pub const CacheEntry = struct {
    key: []const u8,
    value: []const u8,
    checksum: []const u8,
    size: u64,
    created_at: u64,
    accessed_at: u64,
    access_count: u32,
    metadata: std.StringHashMap([]const u8),
    
    pub fn init(allocator: std.mem.Allocator, key: []const u8, value: []const u8) !*CacheEntry {
        var self = try allocator.create(CacheEntry);
        
        // Calculate checksum
        var hasher = std.crypto.hash.sha2.Sha256.init(.{});
        hasher.update(value);
        var checksum_buf: [32]u8 = undefined;
        hasher.final(&checksum_buf);
        
        const timestamp = @as(u64, @intCast(std.time.timestamp()));
        
        self.* = .{
            .key = try allocator.dupe(u8, key),
            .value = try allocator.dupe(u8, value),
            .checksum = try allocator.dupe(u8, &checksum_buf),
            .size = value.len,
            .created_at = timestamp,
            .accessed_at = timestamp,
            .access_count = 0,
            .metadata = std.StringHashMap([]const u8).init(allocator),
        };
        
        return self;
    }
    
    pub fn deinit(self: *CacheEntry, allocator: std.mem.Allocator) void {
        allocator.free(self.key);
        allocator.free(self.value);
        allocator.free(self.checksum);
        
        var iter = self.metadata.iterator();
        while (iter.next()) |entry| {
            allocator.free(entry.key_ptr.*);
            allocator.free(entry.value_ptr.*);
        }
        self.metadata.deinit();
        
        allocator.destroy(self);
    }
    
    pub fn updateAccess(self: *CacheEntry) void {
        self.accessed_at = @as(u64, @intCast(std.time.timestamp()));
        self.access_count += 1;
    }
    
    pub fn verifyIntegrity(self: *CacheEntry) bool {
        var hasher = std.crypto.hash.sha2.Sha256.init(.{});
        hasher.update(self.value);
        var computed_checksum: [32]u8 = undefined;
        hasher.final(&computed_checksum);
        
        return std.mem.eql(u8, self.checksum, &computed_checksum);
    }
    
    pub fn setMetadata(self: *CacheEntry, allocator: std.mem.Allocator, key: []const u8, value: []const u8) !void {
        try self.metadata.put(
            try allocator.dupe(u8, key),
            try allocator.dupe(u8, value)
        );
    }
    
    pub fn getMetadata(self: *CacheEntry, key: []const u8) ?[]const u8 {
        return self.metadata.get(key);
    }
};

pub const CacheStats = struct {
    total_entries: u64,
    total_size: u64,
    hit_count: u64,
    miss_count: u64,
    eviction_count: u64,
    
    pub fn hitRate(self: CacheStats) f64 {
        const total = self.hit_count + self.miss_count;
        if (total == 0) return 0.0;
        return @as(f64, @floatFromInt(self.hit_count)) / @as(f64, @floatFromInt(total));
    }
};

pub const EvictionPolicy = enum {
    lru,    // Least Recently Used
    lfu,    // Least Frequently Used
    fifo,   // First In First Out
    random,
};

pub const Cache = struct {
    allocator: std.mem.Allocator,
    runtime: *tokioZ.Runtime,
    
    // Storage
    entries: std.StringHashMap(*CacheEntry),
    lru_list: std.DoublyLinkedList(*CacheEntry),
    
    // Configuration
    max_size: u64,
    max_entries: u32,
    eviction_policy: EvictionPolicy,
    cache_dir: []const u8,
    
    // Statistics
    stats: CacheStats,
    
    // Concurrency
    mutex: std.Thread.Mutex,
    
    pub fn init(allocator: std.mem.Allocator, runtime: *tokioZ.Runtime, cache_dir: []const u8) !*Cache {
        var self = try allocator.create(Cache);
        self.* = .{
            .allocator = allocator,
            .runtime = runtime,
            .entries = std.StringHashMap(*CacheEntry).init(allocator),
            .lru_list = .{},
            .max_size = 1024 * 1024 * 1024, // 1GB default
            .max_entries = 10000,
            .eviction_policy = .lru,
            .cache_dir = try allocator.dupe(u8, cache_dir),
            .stats = .{
                .total_entries = 0,
                .total_size = 0,
                .hit_count = 0,
                .miss_count = 0,
                .eviction_count = 0,
            },
            .mutex = .{},
        };
        
        // Ensure cache directory exists
        try std.fs.makeDirAbsolute(cache_dir) catch |err| switch (err) {
            error.PathAlreadyExists => {},
            else => return err,
        };
        
        // Load existing cache entries from disk
        try self.loadFromDisk();
        
        return self;
    }
    
    pub fn deinit(self: *Cache) void {
        // Save cache to disk before cleanup
        self.saveToDisk() catch {};
        
        // Clean up entries
        var iter = self.entries.iterator();
        while (iter.next()) |entry| {
            entry.value_ptr.*.deinit(self.allocator);
        }
        self.entries.deinit();
        
        self.allocator.free(self.cache_dir);
        self.allocator.destroy(self);
    }
    
    pub fn get(self: *Cache, key: []const u8) ?[]const u8 {
        self.mutex.lock();
        defer self.mutex.unlock();
        
        if (self.entries.get(key)) |entry| {
            // Verify integrity
            if (!entry.verifyIntegrity()) {
                self.stats.miss_count += 1;
                self.removeEntry(key);
                return null;
            }
            
            entry.updateAccess();
            self.stats.hit_count += 1;
            
            // Move to front of LRU list
            if (self.eviction_policy == .lru) {
                self.updateLruPosition(entry);
            }
            
            return entry.value;
        }
        
        self.stats.miss_count += 1;
        return null;
    }
    
    pub fn put(self: *Cache, key: []const u8, value: []const u8) !void {
        self.mutex.lock();
        defer self.mutex.unlock();
        
        // Check if entry already exists
        if (self.entries.contains(key)) {
            self.removeEntry(key);
        }
        
        const entry = try CacheEntry.init(self.allocator, key, value);
        
        // Check size limits and evict if necessary
        try self.ensureSpace(entry.size);
        
        try self.entries.put(entry.key, entry);
        self.stats.total_entries += 1;
        self.stats.total_size += entry.size;
        
        // Add to LRU list
        const node = try self.allocator.create(std.DoublyLinkedList(*CacheEntry).Node);
        node.data = entry;
        self.lru_list.prepend(node);
        
        // Persist to disk asynchronously
        _ = try self.runtime.spawn(persistEntry, .{ self, entry });
    }
    
    pub fn remove(self: *Cache, key: []const u8) bool {
        self.mutex.lock();
        defer self.mutex.unlock();
        
        return self.removeEntry(key);
    }
    
    pub fn clear(self: *Cache) void {
        self.mutex.lock();
        defer self.mutex.unlock();
        
        var iter = self.entries.iterator();
        while (iter.next()) |entry| {
            entry.value_ptr.*.deinit(self.allocator);
        }
        self.entries.clearAndFree();
        
        // Clear LRU list
        while (self.lru_list.pop()) |node| {
            self.allocator.destroy(node);
        }
        
        self.stats = .{
            .total_entries = 0,
            .total_size = 0,
            .hit_count = 0,
            .miss_count = 0,
            .eviction_count = 0,
        };
    }
    
    pub fn getStats(self: *Cache) CacheStats {
        self.mutex.lock();
        defer self.mutex.unlock();
        return self.stats;
    }
    
    pub fn setMaxSize(self: *Cache, max_size: u64) void {
        self.mutex.lock();
        defer self.mutex.unlock();
        self.max_size = max_size;
    }
    
    pub fn setEvictionPolicy(self: *Cache, policy: EvictionPolicy) void {
        self.mutex.lock();
        defer self.mutex.unlock();
        self.eviction_policy = policy;
    }
    
    // Content-addressable storage methods
    pub fn putByHash(self: *Cache, content: []const u8) ![]const u8 {
        // Generate content hash as key
        var hasher = std.crypto.hash.sha2.Sha256.init(.{});
        hasher.update(content);
        var hash_buf: [32]u8 = undefined;
        hasher.final(&hash_buf);
        
        var hex_buf: [64]u8 = undefined;
        const hex_hash = try std.fmt.bufPrint(&hex_buf, "{}", .{std.fmt.fmtSliceHexLower(&hash_buf)});
        
        try self.put(hex_hash, content);
        return try self.allocator.dupe(u8, hex_hash);
    }
    
    pub fn getByHash(self: *Cache, hash: []const u8) ?[]const u8 {
        return self.get(hash);
    }
    
    // File-based caching
    pub fn cacheFile(self: *Cache, key: []const u8, file_path: []const u8) !void {
        const file = try std.fs.openFileAbsolute(file_path, .{});
        defer file.close();
        
        const content = try file.readToEndAlloc(self.allocator, 100 * 1024 * 1024);
        defer self.allocator.free(content);
        
        try self.put(key, content);
    }
    
    pub fn getCachedFile(self: *Cache, key: []const u8, dest_path: []const u8) !bool {
        if (self.get(key)) |content| {
            const file = try std.fs.createFileAbsolute(dest_path, .{});
            defer file.close();
            
            try file.writeAll(content);
            return true;
        }
        return false;
    }
    
    // Private methods
    fn removeEntry(self: *Cache, key: []const u8) bool {
        if (self.entries.fetchRemove(key)) |kv| {
            const entry = kv.value;
            self.stats.total_entries -= 1;
            self.stats.total_size -= entry.size;
            
            // Remove from LRU list
            self.removeLruNode(entry);
            
            entry.deinit(self.allocator);
            return true;
        }
        return false;
    }
    
    fn ensureSpace(self: *Cache, needed_size: u64) !void {
        while (self.stats.total_size + needed_size > self.max_size or 
               self.stats.total_entries >= self.max_entries) {
            
            try self.evictLeastValuable();
        }
    }
    
    fn evictLeastValuable(self: *Cache) !void {
        const entry = switch (self.eviction_policy) {
            .lru => self.findLruEntry(),
            .lfu => self.findLfuEntry(),
            .fifo => self.findFifoEntry(),
            .random => self.findRandomEntry(),
        } orelse return;
        
        _ = self.removeEntry(entry.key);
        self.stats.eviction_count += 1;
    }
    
    fn findLruEntry(self: *Cache) ?*CacheEntry {
        if (self.lru_list.last) |node| {
            return node.data;
        }
        return null;
    }
    
    fn findLfuEntry(self: *Cache) ?*CacheEntry {
        var min_access_count: u32 = std.math.maxInt(u32);
        var lfu_entry: ?*CacheEntry = null;
        
        var iter = self.entries.iterator();
        while (iter.next()) |kv| {
            const entry = kv.value_ptr.*;
            if (entry.access_count < min_access_count) {
                min_access_count = entry.access_count;
                lfu_entry = entry;
            }
        }
        
        return lfu_entry;
    }
    
    fn findFifoEntry(self: *Cache) ?*CacheEntry {
        var oldest_time: u64 = std.math.maxInt(u64);
        var fifo_entry: ?*CacheEntry = null;
        
        var iter = self.entries.iterator();
        while (iter.next()) |kv| {
            const entry = kv.value_ptr.*;
            if (entry.created_at < oldest_time) {
                oldest_time = entry.created_at;
                fifo_entry = entry;
            }
        }
        
        return fifo_entry;
    }
    
    fn findRandomEntry(self: *Cache) ?*CacheEntry {
        if (self.entries.count() == 0) return null;
        
        var rng = std.rand.DefaultPrng.init(@as(u64, @intCast(std.time.timestamp())));
        const random_index = rng.random().uintLessThan(u32, @intCast(self.entries.count()));
        
        var iter = self.entries.iterator();
        var i: u32 = 0;
        while (iter.next()) |kv| {
            if (i == random_index) {
                return kv.value_ptr.*;
            }
            i += 1;
        }
        
        return null;
    }
    
    fn updateLruPosition(self: *Cache, entry: *CacheEntry) void {
        // Find and move node to front
        var current = self.lru_list.first;
        while (current) |node| {
            if (node.data == entry) {
                self.lru_list.remove(node);
                self.lru_list.prepend(node);
                break;
            }
            current = node.next;
        }
    }
    
    fn removeLruNode(self: *Cache, entry: *CacheEntry) void {
        var current = self.lru_list.first;
        while (current) |node| {
            if (node.data == entry) {
                self.lru_list.remove(node);
                self.allocator.destroy(node);
                break;
            }
            current = node.next;
        }
    }
    
    fn persistEntry(self: *Cache, entry: *CacheEntry) !void {
        const file_path = try std.fs.path.join(self.allocator, &.{ self.cache_dir, entry.key });
        defer self.allocator.free(file_path);
        
        const file = try std.fs.createFileAbsolute(file_path, .{});
        defer file.close();
        
        // Write entry data
        try file.writeAll(entry.value);
        
        // Write metadata
        const meta_path = try std.fmt.allocPrint(self.allocator, "{s}.meta", .{file_path});
        defer self.allocator.free(meta_path);
        
        const meta_file = try std.fs.createFileAbsolute(meta_path, .{});
        defer meta_file.close();
        
        const writer = meta_file.writer();
        try writer.print("checksum={s}\n", .{std.fmt.fmtSliceHexLower(entry.checksum)});
        try writer.print("size={}\n", .{entry.size});
        try writer.print("created_at={}\n", .{entry.created_at});
        try writer.print("accessed_at={}\n", .{entry.accessed_at});
        try writer.print("access_count={}\n", .{entry.access_count});
    }
    
    fn loadFromDisk(self: *Cache) !void {
        var dir = std.fs.openDirAbsolute(self.cache_dir, .{ .iterate = true }) catch return;
        defer dir.close();
        
        var iter = dir.iterate();
        while (try iter.next()) |entry| {
            if (entry.kind != .file or std.mem.endsWith(u8, entry.name, ".meta")) {
                continue;
            }
            
            // Load cache entry from disk
            const file_path = try std.fs.path.join(self.allocator, &.{ self.cache_dir, entry.name });
            defer self.allocator.free(file_path);
            
            const file = std.fs.openFileAbsolute(file_path, .{}) catch continue;
            defer file.close();
            
            const content = file.readToEndAlloc(self.allocator, 100 * 1024 * 1024) catch continue;
            
            const cache_entry = CacheEntry.init(self.allocator, entry.name, content) catch {
                self.allocator.free(content);
                continue;
            };
            
            try self.entries.put(cache_entry.key, cache_entry);
            self.stats.total_entries += 1;
            self.stats.total_size += cache_entry.size;
        }
    }
    
    fn saveToDisk(self: *Cache) !void {
        var iter = self.entries.iterator();
        while (iter.next()) |kv| {
            self.persistEntry(kv.value_ptr.*) catch {};
        }
    }
};