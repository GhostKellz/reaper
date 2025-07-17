# 🛠️ Ghostnet v0.2.1 Compatibility Fix

## Issue Description
The current ghostnet v0.2.1 has a compilation error on Zig 0.15.0-dev where optional types aren't properly unwrapped before field access.

**Error Location:** `src/protocols/http.zig:602:39`
```
error: optional type '?hash_map.HashMapUnmanaged([]const u8,[]const u8,hash_map.StringContext,80).KV' does not support field access
            self.allocator.free(old_kv.key);
                                ~~~~~~^~~~
```

## Root Cause
The `fetchPut` method returns an optional `KV` pair, but the code tries to access `.key` and `.value` directly without unwrapping the optional.

## Fix Required in ghostnet v0.2.2

**File:** `src/protocols/http.zig`
**Lines:** ~598-606 and similar patterns

**Current Code (Broken):**
```zig
pub fn setDefaultHeader(self: *HttpClient, name: []const u8, value: []const u8) !void {
    const name_copy = try self.allocator.dupe(u8, name);
    const value_copy = try self.allocator.dupe(u8, value);
    
    // Free old header if it exists
    if (self.default_headers.fetchPut(name_copy, value_copy)) |old_kv| {
        self.allocator.free(old_kv.key);      // ❌ Error: old_kv is optional
        self.allocator.free(old_kv.value);    // ❌ Error: old_kv is optional
    } else |err| {
        return err;
    }
}
```

**Fixed Code (Working):**
```zig
pub fn setDefaultHeader(self: *HttpClient, name: []const u8, value: []const u8) !void {
    const name_copy = try self.allocator.dupe(u8, name);
    const value_copy = try self.allocator.dupe(u8, value);
    
    // Free old header if it exists
    if (self.default_headers.fetchPut(name_copy, value_copy)) |result| {
        if (result) |old_kv| {              // ✅ Properly unwrap optional
            self.allocator.free(old_kv.key);
            self.allocator.free(old_kv.value);
        }
    } else |err| {
        self.allocator.free(name_copy);     // ✅ Cleanup on error
        self.allocator.free(value_copy);
        return err;
    }
}
```

## Similar Issues to Fix
The same pattern appears in multiple locations:

1. **HttpRequest.setHeader()** - `src/protocols/http.zig:~310`
2. **HttpResponse.setHeader()** - `src/protocols/http.zig:~395`  
3. **Any other fetchPut usage** throughout the codebase

## Verification
After applying the fix, the following should compile successfully:
```bash
zig build
```

## Additional Performance & Security Enhancements for v0.2.2

Based on comprehensive code analysis, here are critical improvements to include:

### **Connection Pool Optimizations**
```zig
// Current pool config is basic - enhance with adaptive sizing
pub const EnhancedPoolConfig = struct {
    // ... existing fields ...
    adaptive_sizing: bool = true,
    per_host_limits: ?std.StringHashMap(u32) = null,
    connection_warming: bool = false,
    dns_cache_ttl: u64 = 300_000_000_000, // 5 minutes
};
```

### **HTTP/2 Performance Issues**
- **Stream Priority**: Current implementation lacks stream prioritization
- **Flow Control**: Window size is static, should be dynamic based on bandwidth
- **Server Push**: PUSH_PROMISE frames are not handled properly

### **Rate Limiting Enhancements**
```zig
// Current rate limiter is basic token bucket - enhance with:
pub const AdvancedRateLimiter = struct {
    per_endpoint_limits: std.StringHashMap(RateLimit),
    sliding_window: bool = true,
    burst_recovery_rate: f64 = 0.1,
    adaptive_limits: bool = false,
};
```

### **Security Gaps to Address**
- **Certificate Pinning**: Missing for production use
- **Request Signing**: No HMAC/signature support for sensitive APIs
- **TLS Verification**: Limited certificate validation options
- **IP Allowlisting**: No connection restriction capabilities

### **Memory Management Issues**
```zig
// Found potential memory leaks in middleware chain
// Fix required in middleware.zig:
pub fn deinit(self: *MiddlewareChain) void {
    for (self.middlewares.items) |*mw| {
        if (mw.cleanup) |cleanup_fn| {
            cleanup_fn(mw);  // ✅ Add cleanup callback
        }
    }
    self.middlewares.deinit();
}
```

### **Error Handling Improvements**  
```zig
// Current HttpError is too basic - enhance with context
pub const EnhancedHttpError = struct {
    kind: ErrorKind,
    message: []const u8,
    url: ?[]const u8,
    status_code: ?u16,
    retry_after: ?u64,
    connection_id: ?u64,
    request_id: ?[]const u8,
    retry_count: u32,
    total_duration: u64,
    
    pub fn toJson(self: *const EnhancedHttpError, allocator: std.mem.Allocator) ![]const u8 {
        // JSON serialization for structured logging
    }
};
```

## Status
- [x] Issue identified in ghostnet v0.2.1
- [x] Performance and security gaps analyzed
- [x] Enhancement roadmap created (see NET_IMPROVEMENTS_WISHLIST.md)
- [ ] Fix implemented in ghostnet v0.2.2 (pending)
- [ ] Performance optimizations implemented (pending)
- [ ] Security enhancements implemented (pending)
- [ ] Reaper updated to use fixed ghostnet version

## Immediate Actions for v0.2.2
1. **Fix compilation error** (blocking)
2. **Memory safety audit** for all HashMap operations
3. **Add connection pool adaptive sizing**
4. **Implement basic certificate pinning**
5. **Enhance error context** for better debugging

## Workaround for Reaper
Until ghostnet v0.2.2 is released, we can:
1. Use a fork of ghostnet with the fix applied
2. Or temporarily disable HTTP client features that use these methods
3. Or vendor the ghostnet dependency and apply the patch locally

## Implementation for v0.2.2
This fix should be part of your ghostnet v0.2.2 release along with the performance and security enhancements outlined in NET_IMPROVEMENTS_WISHLIST.md.
