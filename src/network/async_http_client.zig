const std = @import("std");
const zsync = @import("zsync");

pub const AsyncHttpClientError = error{
    InvalidUrl,
    ConnectionFailed,
    RequestTimeout,
    InvalidResponse,
    TooManyRedirects,
    SslError,
} || std.mem.Allocator.Error;

pub const HttpMethod = enum {
    GET,
    POST,
    PUT,
    DELETE,
    HEAD,
    PATCH,
};

pub const HttpHeader = struct {
    name: []const u8,
    value: []const u8,
};

pub const HttpRequest = struct {
    method: HttpMethod = .GET,
    url: []const u8,
    headers: []const HttpHeader = &.{},
    body: []const u8 = "",
    timeout_ms: u64 = 30000,
    max_redirects: u8 = 5,
    follow_redirects: bool = true,
};

pub const HttpResponse = struct {
    status_code: u16,
    status_text: []const u8,
    headers: []HttpHeader,
    body: []const u8,
    final_url: []const u8,
    redirects: u8,
    total_time_ms: u64,
    allocator: std.mem.Allocator,

    pub fn deinit(self: *HttpResponse) void {
        self.allocator.free(self.status_text);
        for (self.headers) |header| {
            self.allocator.free(header.name);
            self.allocator.free(header.value);
        }
        self.allocator.free(self.headers);
        self.allocator.free(self.body);
        self.allocator.free(self.final_url);
    }

    pub fn isSuccess(self: *const HttpResponse) bool {
        return self.status_code >= 200 and self.status_code < 300;
    }

    pub fn isRedirect(self: *const HttpResponse) bool {
        return self.status_code >= 300 and self.status_code < 400;
    }

    pub fn getHeader(self: *const HttpResponse, name: []const u8) ?[]const u8 {
        for (self.headers) |header| {
            if (std.ascii.eqlIgnoreCase(header.name, name)) {
                return header.value;
            }
        }
        return null;
    }
};

pub const ConnectionPool = struct {
    allocator: std.mem.Allocator,
    io: zsync.Io,
    connections: std.HashMap([]const u8, *Connection, std.StringHashMapContext, std.hash_map.default_max_load_percentage),
    max_connections_per_host: u32,
    keep_alive_timeout_ms: u64,
    mutex: std.Thread.Mutex,

    const Connection = struct {
        stream: zsync.TcpStream,
        host: []const u8,
        port: u16,
        is_ssl: bool,
        last_used: i64,
        requests_sent: u32,
        max_requests: u32,
        in_use: std.atomic.Value(bool),

        pub fn isExpired(self: *const Connection, timeout_ms: u64) bool {
            const now = std.time.milliTimestamp();
            return (now - self.last_used) > timeout_ms;
        }

        pub fn canReuse(self: *const Connection) bool {
            return self.requests_sent < self.max_requests and !self.in_use.load(.acquire);
        }

        pub fn acquire(self: *Connection) bool {
            return !self.in_use.swap(true, .acq_rel);
        }

        pub fn release(self: *Connection) void {
            self.last_used = std.time.milliTimestamp();
            self.in_use.store(false, .release);
        }
    };

    pub fn init(allocator: std.mem.Allocator, io: zsync.Io) ConnectionPool {
        return ConnectionPool{
            .allocator = allocator,
            .io = io,
            .connections = std.HashMap([]const u8, *Connection, std.StringHashMapContext, std.hash_map.default_max_load_percentage).init(allocator),
            .max_connections_per_host = 6,
            .keep_alive_timeout_ms = 300000, // 5 minutes
            .mutex = std.Thread.Mutex{},
        };
    }

    pub fn deinit(self: *ConnectionPool) void {
        self.mutex.lock();
        defer self.mutex.unlock();

        var iterator = self.connections.iterator();
        while (iterator.next()) |entry| {
            const conn = entry.value_ptr.*;
            conn.stream.close() catch {};
            self.allocator.free(conn.host);
            self.allocator.destroy(conn);
        }
        self.connections.deinit();
    }

    fn getConnection(self: *ConnectionPool, host: []const u8, port: u16, is_ssl: bool) !*Connection {
        self.mutex.lock();
        defer self.mutex.unlock();

        const key = try std.fmt.allocPrint(self.allocator, "{s}:{d}:{}", .{ host, port, is_ssl });
        defer self.allocator.free(key);

        // Try to find existing connection
        if (self.connections.get(key)) |conn| {
            if (conn.canReuse() and !conn.isExpired(self.keep_alive_timeout_ms)) {
                if (conn.acquire()) {
                    return conn;
                }
            }
        }

        // Create new connection
        const stream = try self.io.tcp_connect(host, port);
        
        const conn = try self.allocator.create(Connection);
        conn.* = Connection{
            .stream = stream,
            .host = try self.allocator.dupe(u8, host),
            .port = port,
            .is_ssl = is_ssl,
            .last_used = std.time.milliTimestamp(),
            .requests_sent = 0,
            .max_requests = 100,
            .in_use = std.atomic.Value(bool).init(true),
        };

        const stored_key = try self.allocator.dupe(u8, key);
        try self.connections.put(stored_key, conn);

        return conn;
    }

    fn releaseConnection(self: *ConnectionPool, conn: *Connection) void {
        conn.release();
        conn.requests_sent += 1;
    }
};

pub const AsyncHttpClient = struct {
    allocator: std.mem.Allocator,
    io: zsync.Io,
    connection_pool: ConnectionPool,
    user_agent: []const u8,
    default_timeout_ms: u64,

    pub fn init(allocator: std.mem.Allocator, io: zsync.Io) AsyncHttpClient {
        return AsyncHttpClient{
            .allocator = allocator,
            .io = io,
            .connection_pool = ConnectionPool.init(allocator, io),
            .user_agent = "Reaper-AUR-Helper/2.0 (async)",
            .default_timeout_ms = 30000,
        };
    }

    pub fn deinit(self: *AsyncHttpClient) void {
        self.connection_pool.deinit();
    }

    pub fn request(self: *AsyncHttpClient, req: HttpRequest) !HttpResponse {
        const start_time = std.time.milliTimestamp();
        
        // Parse URL
        const uri = std.Uri.parse(req.url) catch return AsyncHttpClientError.InvalidUrl;
        const host = uri.host.?.percent_encoded;
        const is_ssl = std.mem.eql(u8, uri.scheme, "https");
        const port = uri.port orelse if (is_ssl) @as(u16, 443) else @as(u16, 80);
        const path = if (uri.path.percent_encoded.len == 0) "/" else uri.path.percent_encoded;
        const query = if (uri.query) |q| try std.fmt.allocPrint(self.allocator, "?{s}", .{q.percent_encoded}) else "";
        defer if (query.len > 0) self.allocator.free(query);

        var redirects: u8 = 0;
        var current_url = try self.allocator.dupe(u8, req.url);
        defer self.allocator.free(current_url);

        while (redirects <= req.max_redirects) {
            // Get connection from pool
            const conn = try self.connection_pool.getConnection(host, port, is_ssl);
            defer self.connection_pool.releaseConnection(conn);

            // Build HTTP request
            const method_str = switch (req.method) {
                .GET => "GET",
                .POST => "POST",
                .PUT => "PUT",
                .DELETE => "DELETE",
                .HEAD => "HEAD",
                .PATCH => "PATCH",
            };

            var request_builder = std.ArrayList(u8).init(self.allocator);
            defer request_builder.deinit();

            // Request line
            try request_builder.writer().print("{s} {s}{s} HTTP/1.1\r\n", .{ method_str, path, query });
            
            // Headers
            try request_builder.writer().print("Host: {s}\r\n", .{host});
            try request_builder.writer().print("User-Agent: {s}\r\n", .{self.user_agent});
            try request_builder.writer().print("Accept: */*\r\n", .{});
            
            if (req.body.len > 0) {
                try request_builder.writer().print("Content-Length: {d}\r\n", .{req.body.len});
            }
            
            // Connection management
            if (conn.requests_sent < conn.max_requests - 1) {
                try request_builder.writer().print("Connection: keep-alive\r\n", .{});
            } else {
                try request_builder.writer().print("Connection: close\r\n", .{});
            }

            // Custom headers
            for (req.headers) |header| {
                try request_builder.writer().print("{s}: {s}\r\n", .{ header.name, header.value });
            }

            try request_builder.writer().print("\r\n", .{});

            // Body
            if (req.body.len > 0) {
                try request_builder.appendSlice(req.body);
            }

            // Send request
            const request_data = try request_builder.toOwnedSlice();
            defer self.allocator.free(request_data);

            try conn.stream.writeAll(request_data);

            // Read response with timeout
            const response_data = try self.readResponseWithTimeout(conn.stream, req.timeout_ms);
            defer self.allocator.free(response_data);

            // Parse response
            const response = try self.parseResponse(response_data);
            
            // Handle redirects
            if (response.isRedirect() and req.follow_redirects and redirects < req.max_redirects) {
                if (response.getHeader("Location")) |location| {
                    redirects += 1;
                    self.allocator.free(current_url);
                    current_url = try self.resolveRedirectUrl(req.url, location);
                    
                    // Parse new URL and continue loop
                    const new_uri = std.Uri.parse(current_url) catch return AsyncHttpClientError.InvalidUrl;
                    // Update host, port, path etc. for next iteration
                    _ = new_uri; // TODO: Update variables for next iteration
                    continue;
                }
            }

            // Calculate timing
            const total_time = @as(u64, @intCast(std.time.milliTimestamp() - start_time));
            
            return HttpResponse{
                .status_code = response.status_code,
                .status_text = response.status_text,
                .headers = response.headers,
                .body = response.body,
                .final_url = try self.allocator.dupe(u8, current_url),
                .redirects = redirects,
                .total_time_ms = total_time,
                .allocator = self.allocator,
            };
        }

        return AsyncHttpClientError.TooManyRedirects;
    }

    fn readResponseWithTimeout(self: *AsyncHttpClient, stream: zsync.TcpStream, timeout_ms: u64) ![]u8 {
        var response_data = std.ArrayList(u8).init(self.allocator);
        defer response_data.deinit();

        const deadline = std.time.milliTimestamp() + @as(i64, @intCast(timeout_ms));
        var buf: [8192]u8 = undefined;
        var headers_complete = false;
        var content_length: ?usize = null;
        var body_start: usize = 0;

        while (std.time.milliTimestamp() < deadline) {
            const bytes_read = stream.read(&buf) catch |err| switch (err) {
                error.WouldBlock => {
                    // Use zsync v0.4.0 enhanced async wait with timeout
                    try self.io.wait_readable(stream, .{ .timeout_ms = 100, .priority = .normal });
                    continue;
                },
                else => return err,
            };

            if (bytes_read == 0) break;
            try response_data.appendSlice(buf[0..bytes_read]);

            // Check if headers are complete
            if (!headers_complete) {
                if (std.mem.indexOf(u8, response_data.items, "\r\n\r\n")) |header_end| {
                    headers_complete = true;
                    body_start = header_end + 4;
                    
                    // Parse Content-Length if present
                    const headers_section = response_data.items[0..header_end];
                    if (std.mem.indexOf(u8, headers_section, "Content-Length:")) |cl_start| {
                        const line_end = std.mem.indexOf(u8, headers_section[cl_start..], "\r\n") orelse headers_section.len - cl_start;
                        const cl_line = headers_section[cl_start..cl_start + line_end];
                        if (std.mem.indexOf(u8, cl_line, ":")) |colon| {
                            const value = std.mem.trim(u8, cl_line[colon + 1..], " \t");
                            content_length = std.fmt.parseInt(usize, value, 10) catch null;
                        }
                    }
                }
            }

            // Check if we've received the complete body
            if (headers_complete and content_length) |cl| {
                const body_received = response_data.items.len - body_start;
                if (body_received >= cl) {
                    break;
                }
            }
        }

        if (std.time.milliTimestamp() >= deadline) {
            return AsyncHttpClientError.RequestTimeout;
        }

        return response_data.toOwnedSlice();
    }

    fn parseResponse(self: *AsyncHttpClient, data: []const u8) !HttpResponse {
        if (data.len == 0) return AsyncHttpClientError.InvalidResponse;

        // Find header/body boundary
        const header_end = std.mem.indexOf(u8, data, "\r\n\r\n") orelse return AsyncHttpClientError.InvalidResponse;
        const headers_section = data[0..header_end];
        const body_start = header_end + 4;
        const body = data[body_start..];

        // Parse status line
        const first_line_end = std.mem.indexOf(u8, headers_section, "\r\n") orelse return AsyncHttpClientError.InvalidResponse;
        const status_line = headers_section[0..first_line_end];
        
        var status_parts = std.mem.splitScalar(u8, status_line, ' ');
        _ = status_parts.next() orelse return AsyncHttpClientError.InvalidResponse; // HTTP/1.1
        const status_code_str = status_parts.next() orelse return AsyncHttpClientError.InvalidResponse;
        const status_code = std.fmt.parseInt(u16, status_code_str, 10) catch return AsyncHttpClientError.InvalidResponse;
        
        const status_text = status_parts.rest();

        // Parse headers
        var headers = std.ArrayList(HttpHeader).init(self.allocator);
        defer headers.deinit();

        var lines = std.mem.split(u8, headers_section[first_line_end + 2..], "\r\n");
        while (lines.next()) |line| {
            if (line.len == 0) continue;
            
            if (std.mem.indexOf(u8, line, ":")) |colon| {
                const name = std.mem.trim(u8, line[0..colon], " \t");
                const value = std.mem.trim(u8, line[colon + 1..], " \t");
                
                try headers.append(.{
                    .name = try self.allocator.dupe(u8, name),
                    .value = try self.allocator.dupe(u8, value),
                });
            }
        }

        return HttpResponse{
            .status_code = status_code,
            .status_text = try self.allocator.dupe(u8, status_text),
            .headers = try headers.toOwnedSlice(),
            .body = try self.allocator.dupe(u8, body),
            .final_url = "", // Will be set by caller
            .redirects = 0,
            .total_time_ms = 0,
            .allocator = self.allocator,
        };
    }

    fn resolveRedirectUrl(self: *AsyncHttpClient, base_url: []const u8, location: []const u8) ![]u8 {
        // Handle absolute URLs
        if (std.mem.startsWith(u8, location, "http://") or std.mem.startsWith(u8, location, "https://")) {
            return try self.allocator.dupe(u8, location);
        }

        // Handle relative URLs
        const base_uri = std.Uri.parse(base_url) catch return AsyncHttpClientError.InvalidUrl;
        const scheme = base_uri.scheme;
        const host = base_uri.host.?.percent_encoded;
        const port = base_uri.port;

        if (std.mem.startsWith(u8, location, "/")) {
            // Absolute path
            if (port) |p| {
                return try std.fmt.allocPrint(self.allocator, "{s}://{s}:{d}{s}", .{ scheme, host, p, location });
            } else {
                return try std.fmt.allocPrint(self.allocator, "{s}://{s}{s}", .{ scheme, host, location });
            }
        } else {
            // Relative path - simplified implementation
            const base_path = base_uri.path.percent_encoded;
            const last_slash = std.mem.lastIndexOfScalar(u8, base_path, '/') orelse 0;
            const dir_path = base_path[0..last_slash + 1];
            
            if (port) |p| {
                return try std.fmt.allocPrint(self.allocator, "{s}://{s}:{d}{s}{s}", .{ scheme, host, p, dir_path, location });
            } else {
                return try std.fmt.allocPrint(self.allocator, "{s}://{s}{s}{s}", .{ scheme, host, dir_path, location });
            }
        }
    }

    // Convenience methods
    pub fn get(self: *AsyncHttpClient, url: []const u8) !HttpResponse {
        return try self.request(.{ .method = .GET, .url = url });
    }

    pub fn post(self: *AsyncHttpClient, url: []const u8, body: []const u8) !HttpResponse {
        return try self.request(.{ 
            .method = .POST, 
            .url = url, 
            .body = body,
            .headers = &.{
                .{ .name = "Content-Type", .value = "application/x-www-form-urlencoded" },
            },
        });
    }

    pub fn postJson(self: *AsyncHttpClient, url: []const u8, json_body: []const u8) !HttpResponse {
        return try self.request(.{ 
            .method = .POST, 
            .url = url, 
            .body = json_body,
            .headers = &.{
                .{ .name = "Content-Type", .value = "application/json" },
            },
        });
    }
};

// Batch request handling
pub const BatchRequest = struct {
    requests: []const HttpRequest,
    max_concurrent: u32 = 10,
    fail_fast: bool = false,
};

pub const BatchResponse = struct {
    responses: []HttpResponse,
    errors: []?AsyncHttpClientError,
    total_time_ms: u64,
    successful_count: usize,
    allocator: std.mem.Allocator,

    pub fn deinit(self: *BatchResponse) void {
        for (self.responses) |*response| {
            response.deinit();
        }
        self.allocator.free(self.responses);
        self.allocator.free(self.errors);
    }
};

pub fn executeBatch(client: *AsyncHttpClient, batch: BatchRequest) !BatchResponse {
    const start_time = std.time.milliTimestamp();
    var responses = try client.allocator.alloc(HttpResponse, batch.requests.len);
    var errors = try client.allocator.alloc(?AsyncHttpClientError, batch.requests.len);
    
    // Initialize arrays
    for (errors) |*err| {
        err.* = null;
    }

    var successful_count: usize = 0;
    
    // For now, execute sequentially - TODO: implement true concurrency
    for (batch.requests, 0..) |request, i| {
        responses[i] = client.request(request) catch |err| {
            errors[i] = err;
            if (batch.fail_fast) {
                break;
            }
            continue;
        };
        successful_count += 1;
    }

    const total_time = @as(u64, @intCast(std.time.milliTimestamp() - start_time));

    return BatchResponse{
        .responses = responses,
        .errors = errors,
        .total_time_ms = total_time,
        .successful_count = successful_count,
        .allocator = client.allocator,
    };
}