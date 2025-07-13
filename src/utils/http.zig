const std = @import("std");
const tokioZ = @import("tokioZ");

pub const HttpError = error{
    RequestFailed,
    InvalidUrl,
    Timeout,
    ConnectionRefused,
    TooManyRedirects,
};

pub const HttpClient = struct {
    allocator: std.mem.Allocator,
    runtime: *tokioZ.Runtime,
    timeout: u32 = 30000, // 30 seconds
    max_redirects: u32 = 5,
    user_agent: []const u8,
    
    pub fn init(allocator: std.mem.Allocator, runtime: *tokioZ.Runtime) !*HttpClient {
        var self = try allocator.create(HttpClient);
        self.* = .{
            .allocator = allocator,
            .runtime = runtime,
            .user_agent = "reaper/0.1.0",
        };
        return self;
    }
    
    pub fn deinit(self: *HttpClient) void {
        self.allocator.destroy(self);
    }
    
    pub fn get(self: *HttpClient, url: []const u8) !Response {
        return self.request(.GET, url, null);
    }
    
    pub fn post(self: *HttpClient, url: []const u8, body: ?[]const u8) !Response {
        return self.request(.POST, url, body);
    }
    
    pub fn download(self: *HttpClient, url: []const u8, dest_path: []const u8) !void {
        const response = try self.get(url);
        defer response.deinit();
        
        const file = try std.fs.createFileAbsolute(dest_path, .{});
        defer file.close();
        
        try file.writeAll(response.body);
    }
    
    pub fn downloadWithProgress(self: *HttpClient, url: []const u8, dest_path: []const u8, progress_fn: *const fn (downloaded: u64, total: u64) void) !void {
        // Parse URL
        const uri = try std.Uri.parse(url);
        
        // Create connection using tokioZ
        const conn = try self.runtime.connect(uri.host orelse return error.InvalidUrl, uri.port orelse 443);
        defer conn.close();
        
        // Build request
        var request = std.ArrayList(u8).init(self.allocator);
        defer request.deinit();
        
        try request.writer().print(
            \\GET {s} HTTP/1.1
            \\Host: {s}
            \\User-Agent: {s}
            \\Accept: */*
            \\Connection: close
            \\
            \\
        , .{
            uri.path,
            uri.host orelse return error.InvalidUrl,
            self.user_agent,
        });
        
        // Send request
        try conn.writeAll(request.items);
        
        // Read response headers
        var headers = std.ArrayList(u8).init(self.allocator);
        defer headers.deinit();
        
        var buf: [4096]u8 = undefined;
        var total_size: ?u64 = null;
        
        // Parse headers
        while (true) {
            const n = try conn.read(&buf);
            if (n == 0) break;
            
            try headers.appendSlice(buf[0..n]);
            
            if (std.mem.indexOf(u8, headers.items, "\r\n\r\n")) |end| {
                // Parse Content-Length
                if (std.mem.indexOf(u8, headers.items, "Content-Length: ")) |cl_start| {
                    const start = cl_start + 16;
                    if (std.mem.indexOf(u8, headers.items[start..], "\r\n")) |cl_end| {
                        total_size = try std.fmt.parseInt(u64, headers.items[start..start + cl_end], 10);
                    }
                }
                
                // Create file and write body
                const file = try std.fs.createFileAbsolute(dest_path, .{});
                defer file.close();
                
                // Write any body data we already read
                const body_start = end + 4;
                if (body_start < headers.items.len) {
                    try file.writeAll(headers.items[body_start..]);
                    if (total_size) |total| {
                        progress_fn(headers.items.len - body_start, total);
                    }
                }
                
                // Continue reading body
                var downloaded: u64 = headers.items.len - body_start;
                while (true) {
                    const n2 = try conn.read(&buf);
                    if (n2 == 0) break;
                    
                    try file.writeAll(buf[0..n2]);
                    downloaded += n2;
                    
                    if (total_size) |total| {
                        progress_fn(downloaded, total);
                    }
                }
                
                break;
            }
        }
    }
    
    fn request(self: *HttpClient, method: Method, url: []const u8, body: ?[]const u8) !Response {
        // This is a simplified implementation
        // In a real implementation, you would use tokioZ's async HTTP client
        
        var task = try self.runtime.spawn(performRequest, .{ self, method, url, body });
        return task.await();
    }
    
    fn performRequest(self: *HttpClient, method: Method, url: []const u8, body: ?[]const u8) !Response {
        _ = method;
        _ = body;
        
        // For now, use a simplified approach
        // In production, this would use tokioZ's full async capabilities
        
        const allocator = self.allocator;
        var client = std.http.Client{ .allocator = allocator };
        defer client.deinit();
        
        var headers = std.http.Headers{ .allocator = allocator };
        defer headers.deinit();
        
        try headers.append("User-Agent", self.user_agent);
        
        var buf: [8192]u8 = undefined;
        var req = try client.open(.GET, try std.Uri.parse(url), headers, .{});
        defer req.deinit();
        
        try req.send(.{});
        try req.wait();
        
        const body_data = try req.reader().readAllAlloc(allocator, 10 * 1024 * 1024);
        
        return Response{
            .allocator = allocator,
            .status_code = @intFromEnum(req.response.status),
            .headers = try allocator.alloc(Header, 0),
            .body = body_data,
        };
    }
};

pub const Method = enum {
    GET,
    POST,
    PUT,
    DELETE,
    HEAD,
    OPTIONS,
    PATCH,
};

pub const Response = struct {
    allocator: std.mem.Allocator,
    status_code: u16,
    headers: []Header,
    body: []u8,
    
    pub fn deinit(self: *Response) void {
        self.allocator.free(self.body);
        for (self.headers) |header| {
            self.allocator.free(header.name);
            self.allocator.free(header.value);
        }
        self.allocator.free(self.headers);
    }
    
    pub fn isSuccess(self: Response) bool {
        return self.status_code >= 200 and self.status_code < 300;
    }
    
    pub fn getHeader(self: Response, name: []const u8) ?[]const u8 {
        for (self.headers) |header| {
            if (std.ascii.eqlIgnoreCase(header.name, name)) {
                return header.value;
            }
        }
        return null;
    }
};

pub const Header = struct {
    name: []const u8,
    value: []const u8,
};