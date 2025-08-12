const std = @import("std");
const Package = @import("../core/package.zig").Package;

pub const SmartCache = struct {
    allocator: std.mem.Allocator,
    cache_dir: []const u8,
    max_size: u64, // Maximum cache size in bytes
    compression_enabled: bool,
    deduplication_enabled: bool,
    cache_index: CacheIndex,
    
    pub fn init(allocator: std.mem.Allocator, cache_dir: []const u8, max_size: u64) !*SmartCache {
        const self = try allocator.create(SmartCache);
        self.* = .{
            .allocator = allocator,
            .cache_dir = try allocator.dupe(u8, cache_dir),
            .max_size = max_size,
            .compression_enabled = true,
            .deduplication_enabled = true,
            .cache_index = try CacheIndex.init(allocator, cache_dir),
        };
        
        // Create cache directory
        std.fs.makeDirAbsolute(cache_dir) catch {};
        
        // Initialize cache index
        try self.cache_index.load();
        
        std.debug.print("💾 Smart cache initialized: {} (max: {})\n", .{ cache_dir, formatSize(max_size) });
        
        return self;
    }
    
    pub fn deinit(self: *SmartCache) void {
        self.cache_index.deinit();
        self.allocator.free(self.cache_dir);
        self.allocator.destroy(self);
    }
    
    pub fn store(self: *SmartCache, package: Package, file_path: []const u8) !void {
        const cache_entry = try self.createCacheEntry(package, file_path);
        defer cache_entry.deinit(self.allocator);
        
        // Check if we need to make space
        try self.ensureSpace(cache_entry.size);
        
        // Store the file
        const cache_path = try self.getCachePath(cache_entry.hash);
        defer self.allocator.free(cache_path);
        
        if (self.compression_enabled) {
            try self.storeCompressed(file_path, cache_path);
        } else {
            try self.storeDirect(file_path, cache_path);
        }
        
        // Update index
        try self.cache_index.addEntry(cache_entry);
        
        std.debug.print("💾 Cached: {} ({}) -> {}\n", .{ package.name, formatSize(cache_entry.size), cache_entry.hash[0..8] });
    }
    
    pub fn retrieve(self: *SmartCache, package: Package, output_path: []const u8) !bool {
        const package_hash = try self.calculatePackageHash(package);
        defer self.allocator.free(package_hash);
        
        if (try self.cache_index.findEntry(package_hash)) |entry| {
            const cache_path = try self.getCachePath(entry.hash);
            defer self.allocator.free(cache_path);
            
            // Check if cached file exists
            std.fs.accessAbsolute(cache_path, .{}) catch {
                // Cache entry exists but file is missing, remove from index
                try self.cache_index.removeEntry(entry.hash);
                return false;
            };
            
            // Retrieve the file
            if (self.compression_enabled and entry.compressed) {
                try self.retrieveCompressed(cache_path, output_path);
            } else {
                try self.retrieveDirect(cache_path, output_path);
            }
            
            // Update access time
            try self.cache_index.updateAccessTime(entry.hash);
            
            std.debug.print("💾 Cache hit: {} ({})\n", .{ package.name, formatSize(entry.size) });
            return true;
        }
        
        return false;
    }
    
    pub fn clean(self: *SmartCache) !void {
        std.debug.print("🧹 Cleaning cache...\n", .{});
        
        const entries = try self.cache_index.getAllEntries();
        defer self.allocator.free(entries);
        
        var total_cleaned: u64 = 0;
        var files_removed: u32 = 0;
        
        // Remove expired entries
        const now = std.time.timestamp();
        const max_age = 30 * 24 * 60 * 60; // 30 days
        
        for (entries) |entry| {
            if (now - entry.last_accessed > max_age) {
                const cache_path = try self.getCachePath(entry.hash);
                defer self.allocator.free(cache_path);
                
                std.fs.deleteFileAbsolute(cache_path) catch {};
                try self.cache_index.removeEntry(entry.hash);
                
                total_cleaned += entry.size;
                files_removed += 1;
            }
        }
        
        // Remove duplicates (same content, different packages)
        if (self.deduplication_enabled) {
            try self.deduplicateCache();
        }
        
        std.debug.print("🧹 Cleaned {} files ({})\n", .{ files_removed, formatSize(total_cleaned) });
    }
    
    pub fn optimize(self: *SmartCache) !void {
        std.debug.print("⚡ Optimizing cache...\n", .{});
        
        const entries = try self.cache_index.getAllEntries();
        defer self.allocator.free(entries);
        
        var optimized_size: u64 = 0;
        
        // Compress uncompressed files
        if (self.compression_enabled) {
            for (entries) |entry| {
                if (!entry.compressed) {
                    const cache_path = try self.getCachePath(entry.hash);
                    defer self.allocator.free(cache_path);
                    
                    const temp_path = try std.fmt.allocPrint(self.allocator, "{s}.tmp", .{cache_path});
                    defer self.allocator.free(temp_path);
                    
                    const original_size = entry.size;
                    try self.compressFile(cache_path, temp_path);
                    
                    // Replace original with compressed version
                    try std.fs.renameAbsolute(temp_path, cache_path);
                    
                    // Update index
                    var updated_entry = entry;
                    updated_entry.compressed = true;
                    const file = try std.fs.openFileAbsolute(cache_path, .{});
                    defer file.close();
                    const stat = try file.stat();
                    updated_entry.size = stat.size;
                    
                    try self.cache_index.updateEntry(updated_entry);
                    
                    optimized_size += original_size - updated_entry.size;
                }
            }
        }
        
        std.debug.print("⚡ Optimization complete: {} saved\n", .{formatSize(optimized_size)});
    }
    
    pub fn getStats(self: *SmartCache) !CacheStats {
        const entries = try self.cache_index.getAllEntries();
        defer self.allocator.free(entries);
        
        var stats = CacheStats{
            .total_entries = entries.len,
            .total_size = 0,
            .compressed_entries = 0,
            .hit_rate = 0.0,
            .oldest_entry = if (entries.len > 0) entries[0].created else 0,
            .newest_entry = if (entries.len > 0) entries[0].created else 0,
        };
        
        for (entries) |entry| {
            stats.total_size += entry.size;
            if (entry.compressed) stats.compressed_entries += 1;
            if (entry.created < stats.oldest_entry) stats.oldest_entry = entry.created;
            if (entry.created > stats.newest_entry) stats.newest_entry = entry.created;
        }
        
        // Calculate hit rate from index
        stats.hit_rate = self.cache_index.getHitRate();
        
        return stats;
    }
    
    fn createCacheEntry(self: *SmartCache, package: Package, file_path: []const u8) !CacheEntry {
        const file = try std.fs.openFileAbsolute(file_path, .{});
        defer file.close();
        
        const stat = try file.stat();
        const content_hash = try self.calculateFileHash(file);
        const package_hash = try self.calculatePackageHash(package);
        defer self.allocator.free(package_hash);
        
        return CacheEntry{
            .hash = content_hash,
            .package_name = try self.allocator.dupe(u8, package.name),
            .package_version = try self.allocator.dupe(u8, package.version),
            .package_hash = try self.allocator.dupe(u8, package_hash),
            .size = stat.size,
            .compressed = false,
            .created = std.time.timestamp(),
            .last_accessed = std.time.timestamp(),
            .access_count = 1,
        };
    }
    
    fn calculateFileHash(self: *SmartCache, file: std.fs.File) ![32]u8 {
        _ = self;
        var hasher = std.crypto.hash.Blake3.init(.{});
        
        const buffer_size = 8192;
        var buffer: [buffer_size]u8 = undefined;
        
        try file.seekTo(0);
        while (true) {
            const bytes_read = try file.readAll(&buffer);
            if (bytes_read == 0) break;
            hasher.update(buffer[0..bytes_read]);
        }
        
        var hash: [32]u8 = undefined;
        hasher.final(&hash);
        return hash;
    }
    
    fn calculatePackageHash(self: *SmartCache, package: Package) ![]const u8 {
        var hasher = std.crypto.hash.Blake3.init(.{});
        hasher.update(package.name);
        hasher.update(package.version);
        if (package.package_base) |base| hasher.update(base);
        
        var hash: [32]u8 = undefined;
        hasher.final(&hash);
        
        // Convert to hex string
        return try std.fmt.allocPrint(self.allocator, "{}", .{std.fmt.fmtSliceHexLower(&hash)});
    }
    
    fn getCachePath(self: *SmartCache, hash: [32]u8) ![]const u8 {
        const hash_str = try std.fmt.allocPrint(self.allocator, "{}", .{std.fmt.fmtSliceHexLower(&hash)});
        defer self.allocator.free(hash_str);
        
        return try std.fmt.allocPrint(self.allocator, "{s}/{s}", .{ self.cache_dir, hash_str });
    }
    
    fn storeCompressed(self: *SmartCache, source_path: []const u8, dest_path: []const u8) !void {
        try self.compressFile(source_path, dest_path);
    }
    
    fn storeDirect(self: *SmartCache, source_path: []const u8, dest_path: []const u8) !void {
        try std.fs.copyFileAbsolute(source_path, dest_path, .{});
    }
    
    fn retrieveCompressed(self: *SmartCache, source_path: []const u8, dest_path: []const u8) !void {
        try self.decompressFile(source_path, dest_path);
    }
    
    fn retrieveDirect(self: *SmartCache, source_path: []const u8, dest_path: []const u8) !void {
        try std.fs.copyFileAbsolute(source_path, dest_path, .{});
    }
    
    fn compressFile(self: *SmartCache, source_path: []const u8, dest_path: []const u8) !void {
        _ = self;
        // For now, use gzip compression via system command
        // In a real implementation, you'd use a Zig compression library
        
        const compress_cmd = [_][]const u8{ "gzip", "-c", source_path };
        
        const result = try std.process.Child.run(.{
            .allocator = self.allocator,
            .argv = &compress_cmd,
        });
        defer self.allocator.free(result.stdout);
        defer self.allocator.free(result.stderr);
        
        if (result.term != .Exited or result.term.Exited != 0) {
            return error.CompressionFailed;
        }
        
        const dest_file = try std.fs.createFileAbsolute(dest_path, .{});
        defer dest_file.close();
        
        try dest_file.writeAll(result.stdout);
    }
    
    fn decompressFile(self: *SmartCache, source_path: []const u8, dest_path: []const u8) !void {
        _ = self;
        // For now, use gunzip decompression via system command
        
        const decompress_cmd = [_][]const u8{ "gunzip", "-c", source_path };
        
        const result = try std.process.Child.run(.{
            .allocator = self.allocator,
            .argv = &decompress_cmd,
        });
        defer self.allocator.free(result.stdout);
        defer self.allocator.free(result.stderr);
        
        if (result.term != .Exited or result.term.Exited != 0) {
            return error.DecompressionFailed;
        }
        
        const dest_file = try std.fs.createFileAbsolute(dest_path, .{});
        defer dest_file.close();
        
        try dest_file.writeAll(result.stdout);
    }
    
    fn ensureSpace(self: *SmartCache, required_size: u64) !void {
        const current_size = try self.calculateCurrentSize();
        
        if (current_size + required_size <= self.max_size) {
            return; // Enough space available
        }
        
        std.debug.print("💾 Cache full, freeing space...\n", .{});
        
        // Get entries sorted by last access time (LRU)
        const entries = try self.cache_index.getEntriesByLRU();
        defer self.allocator.free(entries);
        
        var freed_size: u64 = 0;
        var files_removed: u32 = 0;
        
        for (entries) |entry| {
            if (current_size - freed_size + required_size <= self.max_size) {
                break; // Enough space freed
            }
            
            const cache_path = try self.getCachePath(entry.hash);
            defer self.allocator.free(cache_path);
            
            std.fs.deleteFileAbsolute(cache_path) catch {};
            try self.cache_index.removeEntry(entry.hash);
            
            freed_size += entry.size;
            files_removed += 1;
        }
        
        std.debug.print("💾 Freed {} ({} files)\n", .{ formatSize(freed_size), files_removed });
    }
    
    fn calculateCurrentSize(self: *SmartCache) !u64 {
        const entries = try self.cache_index.getAllEntries();
        defer self.allocator.free(entries);
        
        var total_size: u64 = 0;
        for (entries) |entry| {
            total_size += entry.size;
        }
        
        return total_size;
    }
    
    fn deduplicateCache(self: *SmartCache) !void {
        std.debug.print("🔗 Deduplicating cache...\n", .{});
        
        const entries = try self.cache_index.getAllEntries();
        defer self.allocator.free(entries);
        
        var content_map = std.HashMap([32]u8, CacheEntry, HashContext, std.hash_map.default_max_load_percentage).init(self.allocator);
        defer content_map.deinit();
        
        var duplicates_removed: u32 = 0;
        var space_saved: u64 = 0;
        
        for (entries) |entry| {
            if (content_map.get(entry.hash)) |existing| {
                // Duplicate found, remove the less recently accessed one
                const to_remove = if (entry.last_accessed < existing.last_accessed) entry else existing;
                const to_keep = if (entry.last_accessed >= existing.last_accessed) entry else existing;
                
                const cache_path = try self.getCachePath(to_remove.hash);
                defer self.allocator.free(cache_path);
                
                std.fs.deleteFileAbsolute(cache_path) catch {};
                try self.cache_index.removeEntry(to_remove.hash);
                
                // Update the map with the kept entry
                try content_map.put(to_keep.hash, to_keep);
                
                duplicates_removed += 1;
                space_saved += to_remove.size;
            } else {
                try content_map.put(entry.hash, entry);
            }
        }
        
        std.debug.print("🔗 Removed {} duplicates ({})\n", .{ duplicates_removed, formatSize(space_saved) });
    }
};

const CacheEntry = struct {
    hash: [32]u8,
    package_name: []const u8,
    package_version: []const u8,
    package_hash: []const u8,
    size: u64,
    compressed: bool,
    created: i64,
    last_accessed: i64,
    access_count: u32,
    
    pub fn deinit(self: *CacheEntry, allocator: std.mem.Allocator) void {
        allocator.free(self.package_name);
        allocator.free(self.package_version);
        allocator.free(self.package_hash);
    }
};

const CacheIndex = struct {
    allocator: std.mem.Allocator,
    index_path: []const u8,
    entries: std.HashMap([32]u8, CacheEntry, HashContext, std.hash_map.default_max_load_percentage),
    total_requests: u64,
    cache_hits: u64,
    
    pub fn init(allocator: std.mem.Allocator, cache_dir: []const u8) !CacheIndex {
        const index_path = try std.fmt.allocPrint(allocator, "{s}/index.json", .{cache_dir});
        
        return CacheIndex{
            .allocator = allocator,
            .index_path = index_path,
            .entries = std.HashMap([32]u8, CacheEntry, HashContext, std.hash_map.default_max_load_percentage).init(allocator),
            .total_requests = 0,
            .cache_hits = 0,
        };
    }
    
    pub fn deinit(self: *CacheIndex) void {
        var iterator = self.entries.iterator();
        while (iterator.next()) |entry| {
            var mut_entry = entry.value_ptr.*;
            mut_entry.deinit(self.allocator);
        }
        self.entries.deinit();
        self.allocator.free(self.index_path);
    }
    
    pub fn load(self: *CacheIndex) !void {
        // Load index from disk (simplified for prototype)
        std.fs.accessAbsolute(self.index_path, .{}) catch {
            // Index doesn't exist, start fresh
            return;
        };
        
        // In a real implementation, this would parse JSON or binary format
        std.debug.print("💾 Loaded cache index\n", .{});
    }
    
    pub fn save(self: *CacheIndex) !void {
        // Save index to disk (simplified for prototype)
        _ = self;
        std.debug.print("💾 Saved cache index\n", .{});
    }
    
    pub fn addEntry(self: *CacheIndex, entry: CacheEntry) !void {
        try self.entries.put(entry.hash, entry);
        try self.save();
    }
    
    pub fn removeEntry(self: *CacheIndex, hash: [32]u8) !void {
        if (self.entries.fetchRemove(hash)) |removed| {
            var mut_entry = removed.value;
            mut_entry.deinit(self.allocator);
        }
        try self.save();
    }
    
    pub fn findEntry(self: *CacheIndex, package_hash: []const u8) !?CacheEntry {
        _ = package_hash;
        // For prototype, this would search by package hash
        // Implementation would need to map package hash to content hash
        
        self.total_requests += 1;
        
        // Simplified: return first entry for demo
        var iterator = self.entries.iterator();
        if (iterator.next()) |entry| {
            self.cache_hits += 1;
            return entry.value_ptr.*;
        }
        
        return null;
    }
    
    pub fn updateAccessTime(self: *CacheIndex, hash: [32]u8) !void {
        if (self.entries.getPtr(hash)) |entry| {
            entry.last_accessed = std.time.timestamp();
            entry.access_count += 1;
            try self.save();
        }
    }
    
    pub fn updateEntry(self: *CacheIndex, updated_entry: CacheEntry) !void {
        try self.entries.put(updated_entry.hash, updated_entry);
        try self.save();
    }
    
    pub fn getAllEntries(self: *CacheIndex) ![]CacheEntry {
        var entries = std.ArrayList(CacheEntry).init(self.allocator);
        
        var iterator = self.entries.iterator();
        while (iterator.next()) |entry| {
            try entries.append(entry.value_ptr.*);
        }
        
        return entries.toOwnedSlice();
    }
    
    pub fn getEntriesByLRU(self: *CacheIndex) ![]CacheEntry {
        var entries = try self.getAllEntries();
        
        // Sort by last accessed time (oldest first)
        std.sort.sort(CacheEntry, entries, {}, struct {
            fn lessThan(context: void, a: CacheEntry, b: CacheEntry) bool {
                _ = context;
                return a.last_accessed < b.last_accessed;
            }
        }.lessThan);
        
        return entries;
    }
    
    pub fn getHitRate(self: *CacheIndex) f32 {
        if (self.total_requests == 0) return 0.0;
        return @as(f32, @floatFromInt(self.cache_hits)) / @as(f32, @floatFromInt(self.total_requests)) * 100.0;
    }
};

const CacheStats = struct {
    total_entries: usize,
    total_size: u64,
    compressed_entries: usize,
    hit_rate: f32,
    oldest_entry: i64,
    newest_entry: i64,
};

const HashContext = struct {
    pub fn hash(self: @This(), key: [32]u8) u64 {
        _ = self;
        var hasher = std.hash.Wyhash.init(0);
        hasher.update(&key);
        return hasher.final();
    }
    
    pub fn eql(self: @This(), a: [32]u8, b: [32]u8) bool {
        _ = self;
        return std.mem.eql(u8, &a, &b);
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