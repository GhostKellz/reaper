const std = @import("std");
const builtin = @import("builtin");

/// Production-ready HTTP client with HTTPS support
pub const HttpClient = struct {
    allocator: std.mem.Allocator,
    timeout_ms: u32 = 30000, // 30 second default timeout
    max_retries: u3 = 3,

    pub fn init(allocator: std.mem.Allocator) *HttpClient {
        const client = allocator.create(HttpClient) catch |err| {
            std.debug.panic("Failed to create HttpClient: {}", .{err});
        };
        client.* = .{
            .allocator = allocator,
        };
        return client;
    }

    pub fn deinit(self: *HttpClient) void {
        self.allocator.destroy(self);
    }

    /// Make a GET request with retry logic
    pub fn get(self: *HttpClient, url: []const u8) !HttpResponse {
        var retry: u3 = 0;
        while (retry < self.max_retries) : (retry += 1) {
            const response = self.getInternal(url) catch |err| {
                if (retry == self.max_retries - 1) return err;
                std.time.sleep(std.time.ns_per_ms * 1000 * @as(u64, retry + 1)); // Exponential backoff
                continue;
            };
            return response;
        }
        unreachable;
    }
    
    fn getInternal(self: *HttpClient, url: []const u8) !HttpResponse {
        const uri = try std.Uri.parse(url);
        const host = uri.host.?.percent_encoded;
        const is_https = std.mem.eql(u8, uri.scheme, "https");
        const port = uri.port orelse if (is_https) @as(u16, 443) else @as(u16, 80);
        const path = if (uri.path.percent_encoded.len == 0) "/" else uri.path.percent_encoded;

        // For HTTPS, we need to use curl as Zig doesn't have native TLS support yet
        if (is_https) {
            return try self.httpsGetWithCurl(url);
        }
        
        // Connect to host for HTTP
        const conn = try std.net.tcpConnectToHost(self.allocator, host, port);
        defer conn.close();
        
        // Socket timeout will be handled by the read timeout below

        // Build HTTP request
        const request = try std.fmt.allocPrint(self.allocator, "GET {s} HTTP/1.1\r\n" ++
            "Host: {s}\r\n" ++
            "User-Agent: Reaper/1.0.4 (AUR helper)\r\n" ++
            "Accept: application/json, */*\r\n" ++
            "Connection: close\r\n" ++
            "\r\n", .{ path, host });
        defer self.allocator.free(request);

        // Send request
        _ = try conn.writeAll(request);

        // Read response
        var response_data = std.ArrayList(u8).init(self.allocator);
        defer response_data.deinit();

        var buf: [8192]u8 = undefined;
        const start_time = std.time.milliTimestamp();
        
        while (true) {
            // Check timeout
            if (std.time.milliTimestamp() - start_time > self.timeout_ms) {
                return error.Timeout;
            }
            
            const bytes_read = conn.read(&buf) catch |err| switch (err) {
                error.WouldBlock => {
                    std.time.sleep(std.time.ns_per_ms * 10);
                    continue;
                },
                else => return err,
            };
            if (bytes_read == 0) break;
            try response_data.appendSlice(buf[0..bytes_read]);
        }

        return try self.parseResponse(response_data.items);
    }

    fn httpsGetWithCurl(self: *HttpClient, url: []const u8) !HttpResponse {
        const max_time_str = try std.fmt.allocPrint(self.allocator, "{}", .{self.timeout_ms / 1000});
        defer self.allocator.free(max_time_str);
        
        
        const result = try std.process.Child.run(.{
            .allocator = self.allocator,
            .argv = &.{ 
                "curl", 
                "-s", 
                "-f", // Fail on HTTP errors
                "-L", // Follow redirects
                "--max-time", max_time_str,
                "--connect-timeout", "5", // Separate connection timeout
                "--compressed", // Accept gzip/deflate
                "-H", "User-Agent: Reaper/2.2.0 (AUR helper)",
                "-H", "Accept: application/json, */*",
                url 
            },
        });
        defer self.allocator.free(result.stderr);
        
        if (result.term != .Exited or result.term.Exited != 0) {
            self.allocator.free(result.stdout);
            return error.HttpRequestFailed;
        }
        
        // Parse HTTP response from curl output (curl returns just the body with -s)
        return HttpResponse{
            .allocator = self.allocator,
            .status_code = 200, // Assume success if curl succeeded
            .body = result.stdout,
        };
    }
    
    fn parseResponse(self: *HttpClient, data: []const u8) !HttpResponse {
        if (data.len == 0) return error.EmptyResponse;
        
        // Find header/body boundary
        const header_end = std.mem.indexOf(u8, data, "\r\n\r\n") orelse {
            // Some servers might use just \n\n
            const alt_end = std.mem.indexOf(u8, data, "\n\n") orelse return error.InvalidResponse;
            const headers_section = data[0..alt_end];
            const body_start = alt_end + 2;
            
            // Parse with \n line endings
            const first_line_end = std.mem.indexOf(u8, headers_section, "\n") orelse return error.InvalidResponse;
            const status_line = headers_section[0..first_line_end];
            
            var status_parts = std.mem.splitScalar(u8, status_line, ' ');
            _ = status_parts.next() orelse return error.InvalidStatusLine; // HTTP/1.1
            const status_str = status_parts.next() orelse return error.InvalidStatusLine;
            const status_code = std.fmt.parseInt(u16, status_str, 10) catch return error.InvalidStatusCode;
            
            const body = if (body_start < data.len) data[body_start..] else "";
            
            return HttpResponse{
                .allocator = self.allocator,
                .status_code = status_code,
                .body = try self.allocator.dupe(u8, body),
            };
        };
        
        const headers_section = data[0..header_end];
        const body_start = header_end + 4;

        // Parse status line
        const first_line_end = std.mem.indexOf(u8, headers_section, "\r\n") orelse return error.InvalidResponse;
        const status_line = headers_section[0..first_line_end];

        var status_parts = std.mem.splitScalar(u8, status_line, ' ');
        _ = status_parts.next() orelse return error.InvalidStatusLine; // HTTP/1.1
        const status_str = status_parts.next() orelse return error.InvalidStatusLine;
        const status_code = std.fmt.parseInt(u16, status_str, 10) catch return error.InvalidStatusCode;

        // Extract body
        const body = if (body_start < data.len) data[body_start..] else "";

        return HttpResponse{
            .allocator = self.allocator,
            .status_code = status_code,
            .body = try self.allocator.dupe(u8, body),
        };
    }
};

pub const HttpResponse = struct {
    allocator: std.mem.Allocator,
    status_code: u16,
    body: []const u8,

    pub fn deinit(self: *HttpResponse) void {
        self.allocator.free(self.body);
    }

    pub fn isSuccess(self: *const HttpResponse) bool {
        return self.status_code >= 200 and self.status_code < 300;
    }
};

// AUR-specific helper functions
pub fn aurSearch(client: *HttpClient, query: []const u8) !HttpResponse {
    const url = try std.fmt.allocPrint(client.allocator, "https://aur.archlinux.org/rpc/?v=5&type=search&arg={s}", .{query});
    defer client.allocator.free(url);

    return try client.get(url);
}

pub fn aurInfo(client: *HttpClient, package_name: []const u8) !HttpResponse {
    const url = try std.fmt.allocPrint(client.allocator, "https://aur.archlinux.org/rpc/?v=5&type=info&arg[]={s}", .{package_name});
    defer client.allocator.free(url);

    return try client.get(url);
}
