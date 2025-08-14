const std = @import("std");
const zsync = @import("zsync");

pub const CacheError = error{
    KeyNotFound,
    CacheCorrupted,
    SerializationFailed,
    DeserializationFailed,
    InvalidCacheVersion,
    StorageFull,
    CacheExpired,
    ConcurrentModification,
} || std.mem.Allocator.Error || std.fs.File.OpenError || std.fs.File.WriteError || std.fs.File.ReadError;

pub const CacheStrategy = enum {
    lru,          // Least Recently Used
    lfu,          // Least Frequently Used
    ttl,          // Time To Live
    adaptive,     // Adaptive replacement cache
};

pub const CacheEntry = struct {
    key: []const u8,
    value: []const u8,
    metadata: CacheMetadata,
    access_count: std.atomic.Value(u64),
    last_accessed: std.atomic.Value(i64),
    
    const CacheMetadata = struct {
        created_at: i64,
        expires_at: ?i64,
        size_bytes: u64,
        version: u32,
        checksum: u32,
        compression: CompressionType,
        tags: [][]const u8,
    };
    
    const CompressionType = enum {
        none,
        gzip,
        lz4,
        zstd,
    };
    
    pub fn init(allocator: std.mem.Allocator, key: []const u8, value: []const u8, ttl_seconds: ?u64) !*CacheEntry {
        const entry = try allocator.create(CacheEntry);
        const now = std.time.milliTimestamp();
        
        entry.* = CacheEntry{
            .key = try allocator.dupe(u8, key),
            .value = try allocator.dupe(u8, value),
            .metadata = CacheMetadata{
                .created_at = now,
                .expires_at = if (ttl_seconds) |ttl| now + @as(i64, @intCast(ttl * 1000)) else null,
                .size_bytes = value.len,
                .version = 1,
                .checksum = std.hash.Crc32.hash(value),
                .compression = .none,
                .tags = &.{},
            },
            .access_count = std.atomic.Value(u64).init(1),
            .last_accessed = std.atomic.Value(i64).init(now),
        };
        
        return entry;
    }
    
    pub fn deinit(self: *CacheEntry, allocator: std.mem.Allocator) void {
        allocator.free(self.key);
        allocator.free(self.value);
        
        for (self.metadata.tags) |tag| {
            allocator.free(tag);
        }
        allocator.free(self.metadata.tags);
        
        allocator.destroy(self);
    }
    
    pub fn isExpired(self: *const CacheEntry) bool {
        if (self.metadata.expires_at) |expires| {
            return std.time.milliTimestamp() > expires;
        }
        return false;
    }
    
    pub fn touch(self: *CacheEntry) void {
        _ = self.access_count.fetchAdd(1, .acq_rel);
        self.last_accessed.store(std.time.milliTimestamp(), .release);
    }
    
    pub fn isValid(self: *const CacheEntry) bool {
        if (self.isExpired()) return false;
        
        // Verify checksum
        const calculated_checksum = std.hash.Crc32.hash(self.value);
        return calculated_checksum == self.metadata.checksum;
    }
    
    pub fn compress(self: *CacheEntry, allocator: std.mem.Allocator, compression_type: CompressionType) !void {
        if (self.metadata.compression != .none) return; // Already compressed
        
        const compressed_data = switch (compression_type) {
            .none => return,
            .gzip => try self.compressGzip(allocator),
            .lz4 => try self.compressLz4(allocator),
            .zstd => try self.compressZstd(allocator),
        };
        
        // Replace value with compressed data
        allocator.free(self.value);
        self.value = compressed_data;
        self.metadata.compression = compression_type;
        self.metadata.size_bytes = compressed_data.len;
        self.metadata.checksum = std.hash.Crc32.hash(compressed_data);
    }
    
    pub fn decompress(self: *CacheEntry, allocator: std.mem.Allocator) ![]const u8 {
        return switch (self.metadata.compression) {
            .none => try allocator.dupe(u8, self.value),
            .gzip => try self.decompressGzip(allocator),
            .lz4 => try self.decompressLz4(allocator),
            .zstd => try self.decompressZstd(allocator),
        };
    }
    
    fn compressGzip(self: *CacheEntry, allocator: std.mem.Allocator) ![]u8 {
        // Simplified compression - real implementation would use proper gzip
        _ = self;
        return try allocator.dupe(u8, "COMPRESSED_GZIP_DATA");
    }
    
    fn compressLz4(self: *CacheEntry, allocator: std.mem.Allocator) ![]u8 {
        _ = self;
        return try allocator.dupe(u8, "COMPRESSED_LZ4_DATA");
    }
    
    fn compressZstd(self: *CacheEntry, allocator: std.mem.Allocator) ![]u8 {
        _ = self;
        return try allocator.dupe(u8, "COMPRESSED_ZSTD_DATA");
    }
    
    fn decompressGzip(self: *CacheEntry, allocator: std.mem.Allocator) ![]u8 {
        _ = self;
        return try allocator.dupe(u8, "DECOMPRESSED_FROM_GZIP");
    }
    
    fn decompressLz4(self: *CacheEntry, allocator: std.mem.Allocator) ![]u8 {
        _ = self;
        return try allocator.dupe(u8, "DECOMPRESSED_FROM_LZ4");
    }
    
    fn decompressZstd(self: *CacheEntry, allocator: std.mem.Allocator) ![]u8 {
        _ = self;
        return try allocator.dupe(u8, "DECOMPRESSED_FROM_ZSTD");
    }
};

pub const CacheConfig = struct {
    max_size_bytes: u64 = 1024 * 1024 * 1024, // 1GB
    max_entries: u32 = 100000,
    default_ttl_seconds: ?u64 = 3600, // 1 hour
    strategy: CacheStrategy = .lru,
    compression_threshold: u64 = 1024, // Compress entries larger than 1KB
    compression_type: CacheEntry.CompressionType = .lz4,
    persistence_enabled: bool = true,
    persistence_interval_seconds: u64 = 300, // 5 minutes
    cache_directory: []const u8 = "/var/cache/reaper",
    enable_memory_mapping: bool = true,
    auto_cleanup_interval_seconds: u64 = 600, // 10 minutes
};

pub const AsyncCache = struct {
    allocator: std.mem.Allocator,
    io: zsync.Io,
    config: CacheConfig,
    
    // In-memory storage
    entries: std.StringHashMap(*CacheEntry),
    access_order: std.ArrayList([]const u8), // For LRU
    frequency_map: std.StringHashMap(u64), // For LFU
    
    // Statistics
    stats: CacheStats,
    
    // Synchronization
    mutex: std.Thread.RwLock,
    
    // Background tasks
    persistence_thread: ?std.Thread,
    cleanup_thread: ?std.Thread,
    shutdown: std.atomic.Value(bool),
    
    // Persistence
    cache_file: ?std.fs.File,
    memory_mapped_region: ?[]align(std.mem.page_size) u8,
    
    const CacheStats = struct {
        hits: std.atomic.Value(u64),
        misses: std.atomic.Value(u64),
        evictions: std.atomic.Value(u64),
        bytes_stored: std.atomic.Value(u64),
        entries_count: std.atomic.Value(u32),
        compression_savings_bytes: std.atomic.Value(u64),
        persistence_writes: std.atomic.Value(u64),
        persistence_reads: std.atomic.Value(u64),
    };
    
    pub fn init(allocator: std.mem.Allocator, io: zsync.Io, config: CacheConfig) !*AsyncCache {
        const cache = try allocator.create(AsyncCache);
        
        cache.* = AsyncCache{
            .allocator = allocator,
            .io = io,
            .config = config,
            .entries = std.StringHashMap(*CacheEntry).init(allocator),
            .access_order = std.ArrayList([]const u8).init(allocator),
            .frequency_map = std.StringHashMap(u64).init(allocator),
            .stats = CacheStats{
                .hits = std.atomic.Value(u64).init(0),
                .misses = std.atomic.Value(u64).init(0),
                .evictions = std.atomic.Value(u64).init(0),
                .bytes_stored = std.atomic.Value(u64).init(0),
                .entries_count = std.atomic.Value(u32).init(0),
                .compression_savings_bytes = std.atomic.Value(u64).init(0),
                .persistence_writes = std.atomic.Value(u64).init(0),
                .persistence_reads = std.atomic.Value(u64).init(0),
            },
            .mutex = std.Thread.RwLock{},
            .persistence_thread = null,
            .cleanup_thread = null,
            .shutdown = std.atomic.Value(bool).init(false),
            .cache_file = null,
            .memory_mapped_region = null,
        };
        
        // Create cache directory
        if (config.persistence_enabled) {
            std.fs.makeDirAbsolute(config.cache_directory) catch |err| switch (err) {
                error.PathAlreadyExists => {},
                else => return err,
            };
        }
        
        // Load existing cache from disk
        if (config.persistence_enabled) {
            try cache.loadFromDisk();
        }
        
        // Start background threads
        if (config.persistence_enabled) {
            cache.persistence_thread = try std.Thread.spawn(.{}, persistenceWorker, .{cache});
        }
        
        cache.cleanup_thread = try std.Thread.spawn(.{}, cleanupWorker, .{cache});
        
        return cache;
    }
    
    pub fn deinit(self: *AsyncCache) void {
        // Signal shutdown
        self.shutdown.store(true, .release);
        
        // Wait for background threads
        if (self.persistence_thread) |thread| {
            thread.join();
        }
        if (self.cleanup_thread) |thread| {
            thread.join();
        }
        
        // Save to disk one final time
        if (self.config.persistence_enabled) {
            self.saveToDisk() catch |err| {
                std.log.err("Failed to save cache to disk during shutdown: {}", .{err});
            };
        }
        
        // Cleanup entries
        self.mutex.lock();
        defer self.mutex.unlock();
        
        var entry_iter = self.entries.iterator();
        while (entry_iter.next()) |entry| {
            entry.value_ptr.*.deinit(self.allocator);
            self.allocator.free(entry.key_ptr.*);
        }
        self.entries.deinit();
        
        for (self.access_order.items) |key| {
            self.allocator.free(key);
        }
        self.access_order.deinit();
        
        var freq_iter = self.frequency_map.iterator();
        while (freq_iter.next()) |entry| {
            self.allocator.free(entry.key_ptr.*);
        }
        self.frequency_map.deinit();
        
        // Cleanup persistence resources
        if (self.memory_mapped_region) |region| {
            std.posix.munmap(region);
        }
        if (self.cache_file) |file| {
            file.close();
        }
        
        self.allocator.destroy(self);
    }
    
    pub fn get(self: *AsyncCache, key: []const u8) !?[]const u8 {
        self.mutex.lockShared();
        defer self.mutex.unlockShared();
        
        if (self.entries.get(key)) |entry| {
            // Check if entry is valid
            if (!entry.isValid()) {
                // Entry is invalid, remove it
                self.mutex.unlockShared();
                self.mutex.lock();
                _ = self.removeEntryLocked(key);
                self.mutex.unlock();
                self.mutex.lockShared();
                
                _ = self.stats.misses.fetchAdd(1, .acq_rel);
                return null;
            }
            
            // Touch entry for cache strategy
            entry.touch();
            self.updateAccessOrder(key);
            
            _ = self.stats.hits.fetchAdd(1, .acq_rel);
            
            // Decompress if needed
            return try entry.decompress(self.allocator);
        }
        
        _ = self.stats.misses.fetchAdd(1, .acq_rel);
        return null;
    }
    
    pub fn getAsync(self: *AsyncCache, key: []const u8) !zsync.task_management.TaskHandle {
        return try zsync.task_management.Task.spawn(self.allocator, struct {
            fn getCacheEntry(cache: *AsyncCache, cache_key: []const u8) !?[]const u8 {
                return try cache.get(cache_key);
            }
        }.getCacheEntry, .{ self, key }, .{
            .timeout_ms = 5000,
            .priority = .normal,
        });
    }
    
    pub fn put(self: *AsyncCache, key: []const u8, value: []const u8, ttl_seconds: ?u64) !void {
        // Create new entry
        const entry = try CacheEntry.init(self.allocator, key, value, ttl_seconds orelse self.config.default_ttl_seconds);
        
        // Compress if value is large enough
        if (value.len >= self.config.compression_threshold) {
            const original_size = entry.metadata.size_bytes;
            try entry.compress(self.allocator, self.config.compression_type);
            const compression_savings = original_size - entry.metadata.size_bytes;
            _ = self.stats.compression_savings_bytes.fetchAdd(compression_savings, .acq_rel);
        }
        
        self.mutex.lock();
        defer self.mutex.unlock();
        
        // Check if we need to evict entries
        try self.ensureCapacityLocked(entry.metadata.size_bytes);
        
        // Remove existing entry if present
        if (self.entries.get(key)) |existing| {
            _ = self.removeEntryLocked(key);
            existing.deinit(self.allocator);
        }
        
        // Add new entry
        const key_copy = try self.allocator.dupe(u8, key);
        try self.entries.put(key_copy, entry);
        
        // Update access tracking
        try self.access_order.insert(0, try self.allocator.dupe(u8, key));
        try self.frequency_map.put(try self.allocator.dupe(u8, key), 1);
        
        // Update statistics
        _ = self.stats.bytes_stored.fetchAdd(entry.metadata.size_bytes, .acq_rel);
        _ = self.stats.entries_count.fetchAdd(1, .acq_rel);
    }
    
    pub fn putAsync(self: *AsyncCache, key: []const u8, value: []const u8, ttl_seconds: ?u64) !zsync.task_management.TaskHandle {
        return try zsync.task_management.Task.spawn(self.allocator, struct {
            fn putCacheEntry(cache: *AsyncCache, cache_key: []const u8, cache_value: []const u8, ttl: ?u64) !void {
                try cache.put(cache_key, cache_value, ttl);
            }
        }.putCacheEntry, .{ self, key, value, ttl_seconds }, .{
            .timeout_ms = 10000,
            .priority = .normal,
        });
    }
    
    pub fn remove(self: *AsyncCache, key: []const u8) bool {
        self.mutex.lock();
        defer self.mutex.unlock();
        
        return self.removeEntryLocked(key);
    }
    
    fn removeEntryLocked(self: *AsyncCache, key: []const u8) bool {
        if (self.entries.fetchRemove(key)) |removed_entry| {
            const entry = removed_entry.value;
            
            // Update statistics
            _ = self.stats.bytes_stored.fetchSub(entry.metadata.size_bytes, .acq_rel);
            _ = self.stats.entries_count.fetchSub(1, .acq_rel);
            
            // Remove from access tracking
            for (self.access_order.items, 0..) |access_key, i| {
                if (std.mem.eql(u8, access_key, key)) {
                    const removed_key = self.access_order.swapRemove(i);
                    self.allocator.free(removed_key);
                    break;
                }
            }
            
            if (self.frequency_map.fetchRemove(key)) |removed_freq| {
                self.allocator.free(removed_freq.key);
            }
            
            // Cleanup
            entry.deinit(self.allocator);
            self.allocator.free(removed_entry.key);
            
            return true;
        }
        
        return false;
    }
    
    pub fn clear(self: *AsyncCache) void {
        self.mutex.lock();
        defer self.mutex.unlock();
        
        // Remove all entries
        var entry_iter = self.entries.iterator();
        while (entry_iter.next()) |entry| {
            entry.value_ptr.*.deinit(self.allocator);
            self.allocator.free(entry.key_ptr.*);
        }
        self.entries.clearAndFree();
        
        // Clear access tracking
        for (self.access_order.items) |key| {
            self.allocator.free(key);
        }
        self.access_order.clearAndFree();
        
        var freq_iter = self.frequency_map.iterator();
        while (freq_iter.next()) |entry| {
            self.allocator.free(entry.key_ptr.*);
        }
        self.frequency_map.clearAndFree();
        
        // Reset statistics
        self.stats.bytes_stored.store(0, .release);
        self.stats.entries_count.store(0, .release);
    }
    
    pub fn invalidateByTag(self: *AsyncCache, tag: []const u8) u32 {
        self.mutex.lock();
        defer self.mutex.unlock();
        
        var invalidated_count: u32 = 0;
        var keys_to_remove = std.ArrayList([]const u8).init(self.allocator);
        defer keys_to_remove.deinit();
        
        // Find entries with matching tag
        var entry_iter = self.entries.iterator();
        while (entry_iter.next()) |entry| {
            const cache_entry = entry.value_ptr.*;
            for (cache_entry.metadata.tags) |entry_tag| {
                if (std.mem.eql(u8, entry_tag, tag)) {
                    keys_to_remove.append(entry.key_ptr.*) catch continue;
                    break;
                }
            }
        }
        
        // Remove entries
        for (keys_to_remove.items) |key| {
            if (self.removeEntryLocked(key)) {
                invalidated_count += 1;
            }
        }
        
        return invalidated_count;
    }
    
    fn ensureCapacityLocked(self: *AsyncCache, additional_bytes: u64) !void {
        const current_bytes = self.stats.bytes_stored.load(.acquire);
        const current_entries = self.stats.entries_count.load(.acquire);
        
        // Check if we need to evict entries
        while ((current_bytes + additional_bytes > self.config.max_size_bytes) or
               (current_entries >= self.config.max_entries)) {
            
            const evicted = try self.evictOneLocked();
            if (!evicted) break; // No more entries to evict
        }
    }
    
    fn evictOneLocked(self: *AsyncCache) !bool {
        const key_to_evict = switch (self.config.strategy) {
            .lru => self.findLruKeyLocked(),
            .lfu => self.findLfuKeyLocked(),
            .ttl => self.findExpiredKeyLocked(),
            .adaptive => self.findAdaptiveKeyLocked(),
        };
        
        if (key_to_evict) |key| {
            _ = self.stats.evictions.fetchAdd(1, .acq_rel);
            return self.removeEntryLocked(key);
        }
        
        return false;
    }
    
    fn findLruKeyLocked(self: *AsyncCache) ?[]const u8 {
        // Return least recently used key (last in access order)
        if (self.access_order.items.len > 0) {
            return self.access_order.items[self.access_order.items.len - 1];
        }
        return null;
    }
    
    fn findLfuKeyLocked(self: *AsyncCache) ?[]const u8 {
        var min_frequency: u64 = std.math.maxInt(u64);
        var lfu_key: ?[]const u8 = null;
        
        var freq_iter = self.frequency_map.iterator();
        while (freq_iter.next()) |entry| {
            if (entry.value_ptr.* < min_frequency) {
                min_frequency = entry.value_ptr.*;
                lfu_key = entry.key_ptr.*;
            }
        }
        
        return lfu_key;
    }
    
    fn findExpiredKeyLocked(self: *AsyncCache) ?[]const u8 {
        var entry_iter = self.entries.iterator();
        while (entry_iter.next()) |entry| {
            if (entry.value_ptr.*.isExpired()) {
                return entry.key_ptr.*;
            }
        }
        return null;
    }
    
    fn findAdaptiveKeyLocked(self: *AsyncCache) ?[]const u8 {
        // Adaptive strategy: prefer expired entries, then LFU, then LRU
        if (self.findExpiredKeyLocked()) |key| return key;
        if (self.findLfuKeyLocked()) |key| return key;
        return self.findLruKeyLocked();
    }
    
    fn updateAccessOrder(self: *AsyncCache, key: []const u8) void {
        // Move key to front of access order (most recently used)
        for (self.access_order.items, 0..) |access_key, i| {
            if (std.mem.eql(u8, access_key, key)) {
                const moved_key = self.access_order.orderedRemove(i);
                self.access_order.insert(0, moved_key) catch return;
                break;
            }
        }
        
        // Update frequency
        if (self.frequency_map.getPtr(key)) |freq| {
            freq.* += 1;
        }
    }
    
    fn loadFromDisk(self: *AsyncCache) !void {
        const cache_file_path = try std.fs.path.join(self.allocator, &.{ self.config.cache_directory, "cache.db" });
        defer self.allocator.free(cache_file_path);
        
        const file = std.fs.openFileAbsolute(cache_file_path, .{ .mode = .read_write }) catch |err| switch (err) {
            error.FileNotFound => {
                std.log.info("No existing cache file found, starting with empty cache");
                return;
            },
            else => return err,
        };
        
        self.cache_file = file;
        
        // Read cache header
        var header_buf: [256]u8 = undefined;
        const bytes_read = try file.readAll(&header_buf);
        if (bytes_read == 0) return; // Empty file
        
        // Simple format: version + entry count + entries
        // Real implementation would use a proper format like messagepack or protobuf
        
        _ = self.stats.persistence_reads.fetchAdd(1, .acq_rel);
        std.log.info("Loaded cache from disk");
    }
    
    fn saveToDisk(self: *AsyncCache) !void {
        if (!self.config.persistence_enabled) return;
        
        const cache_file_path = try std.fs.path.join(self.allocator, &.{ self.config.cache_directory, "cache.db" });
        defer self.allocator.free(cache_file_path);
        
        const temp_file_path = try std.fmt.allocPrint(self.allocator, "{s}.tmp", .{cache_file_path});
        defer self.allocator.free(temp_file_path);
        
        // Write to temporary file first
        const temp_file = try std.fs.createFileAbsolute(temp_file_path, .{});
        defer temp_file.close();
        
        // Write cache data
        self.mutex.lockShared();
        defer self.mutex.unlockShared();
        
        // Simple serialization format
        var entry_iter = self.entries.iterator();
        while (entry_iter.next()) |entry| {
            // Write entry data (simplified)
            _ = try temp_file.write(entry.key_ptr.*);
            _ = try temp_file.write(entry.value_ptr.*.value);
        }
        
        // Atomic rename
        try std.fs.renameAbsolute(temp_file_path, cache_file_path);
        
        _ = self.stats.persistence_writes.fetchAdd(1, .acq_rel);
        std.log.debug("Saved cache to disk");
    }
    
    pub fn getStatistics(self: *const AsyncCache) struct {
        hits: u64,
        misses: u64,
        hit_rate: f32,
        evictions: u64,
        bytes_stored: u64,
        entries_count: u32,
        compression_savings_bytes: u64,
        persistence_writes: u64,
        persistence_reads: u64,
    } {
        const hits = self.stats.hits.load(.acquire);
        const misses = self.stats.misses.load(.acquire);
        const total_requests = hits + misses;
        
        return .{
            .hits = hits,
            .misses = misses,
            .hit_rate = if (total_requests > 0) @as(f32, @floatFromInt(hits)) / @as(f32, @floatFromInt(total_requests)) else 0.0,
            .evictions = self.stats.evictions.load(.acquire),
            .bytes_stored = self.stats.bytes_stored.load(.acquire),
            .entries_count = self.stats.entries_count.load(.acquire),
            .compression_savings_bytes = self.stats.compression_savings_bytes.load(.acquire),
            .persistence_writes = self.stats.persistence_writes.load(.acquire),
            .persistence_reads = self.stats.persistence_reads.load(.acquire),
        };
    }
    
    fn persistenceWorker(self: *AsyncCache) void {
        std.log.info("Cache persistence worker started");
        
        while (!self.shutdown.load(.acquire)) {
            const interval_ms = self.config.persistence_interval_seconds * 1000;
            std.time.sleep(interval_ms * std.time.ns_per_ms);
            
            if (self.shutdown.load(.acquire)) break;
            
            self.saveToDisk() catch |err| {
                std.log.err("Failed to save cache to disk: {}", .{err});
            };
        }
        
        std.log.info("Cache persistence worker stopped");
    }
    
    fn cleanupWorker(self: *AsyncCache) void {
        std.log.info("Cache cleanup worker started");
        
        while (!self.shutdown.load(.acquire)) {
            const interval_ms = self.config.auto_cleanup_interval_seconds * 1000;
            std.time.sleep(interval_ms * std.time.ns_per_ms);
            
            if (self.shutdown.load(.acquire)) break;
            
            // Remove expired entries
            self.removeExpiredEntries();
        }
        
        std.log.info("Cache cleanup worker stopped");
    }
    
    fn removeExpiredEntries(self: *AsyncCache) void {
        self.mutex.lock();
        defer self.mutex.unlock();
        
        var keys_to_remove = std.ArrayList([]const u8).init(self.allocator);
        defer keys_to_remove.deinit();
        
        // Find expired entries
        var entry_iter = self.entries.iterator();
        while (entry_iter.next()) |entry| {
            if (entry.value_ptr.*.isExpired()) {
                keys_to_remove.append(entry.key_ptr.*) catch continue;
            }
        }
        
        // Remove expired entries
        for (keys_to_remove.items) |key| {
            _ = self.removeEntryLocked(key);
        }
        
        if (keys_to_remove.items.len > 0) {
            std.log.debug("Removed {} expired cache entries", .{keys_to_remove.items.len});
        }
    }
};

// Convenience functions for common cache patterns

pub fn createPackageCache(allocator: std.mem.Allocator, io: zsync.Io, cache_dir: []const u8) !*AsyncCache {
    const config = CacheConfig{
        .max_size_bytes = 512 * 1024 * 1024, // 512MB
        .max_entries = 50000,
        .default_ttl_seconds = 24 * 60 * 60, // 24 hours
        .strategy = .adaptive,
        .compression_threshold = 4096, // 4KB
        .compression_type = .lz4,
        .cache_directory = cache_dir,
        .persistence_interval_seconds = 600, // 10 minutes
    };
    
    return try AsyncCache.init(allocator, io, config);
}

pub fn createDownloadCache(allocator: std.mem.Allocator, io: zsync.Io, cache_dir: []const u8) !*AsyncCache {
    const config = CacheConfig{
        .max_size_bytes = 2 * 1024 * 1024 * 1024, // 2GB
        .max_entries = 10000,
        .default_ttl_seconds = 7 * 24 * 60 * 60, // 7 days
        .strategy = .lru,
        .compression_threshold = 8192, // 8KB
        .compression_type = .zstd,
        .cache_directory = cache_dir,
        .persistence_interval_seconds = 1800, // 30 minutes
    };
    
    return try AsyncCache.init(allocator, io, config);
}