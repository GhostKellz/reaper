const std = @import("std");

pub const TomlError = error{
    InvalidSyntax,
    UnexpectedCharacter,
    InvalidKey,
    InvalidValue,
    DuplicateKey,
};

pub const TomlValue = union(enum) {
    string: []const u8,
    integer: i64,
    float: f64,
    boolean: bool,
    array: []TomlValue,
    table: std.StringHashMap(TomlValue),
    
    pub fn deinit(self: *TomlValue, allocator: std.mem.Allocator) void {
        switch (self.*) {
            .string => |s| allocator.free(s),
            .array => |arr| {
                for (arr) |*item| {
                    item.deinit(allocator);
                }
                allocator.free(arr);
            },
            .table => |*map| {
                var iter = map.iterator();
                while (iter.next()) |entry| {
                    allocator.free(entry.key_ptr.*);
                    entry.value_ptr.deinit(allocator);
                }
                map.deinit();
            },
            else => {},
        }
    }
};

pub const TomlParser = struct {
    allocator: std.mem.Allocator,
    source: []const u8,
    pos: usize = 0,
    
    pub fn init(allocator: std.mem.Allocator, source: []const u8) TomlParser {
        return .{
            .allocator = allocator,
            .source = source,
        };
    }
    
    pub fn parse(self: *TomlParser) !TomlValue {
        var root = std.StringHashMap(TomlValue).init(self.allocator);
        errdefer root.deinit();
        
        var current_table: ?[]const u8 = null;
        
        while (self.pos < self.source.len) {
            self.skipWhitespaceAndComments();
            if (self.pos >= self.source.len) break;
            
            const ch = self.source[self.pos];
            
            if (ch == '[') {
                // Table header
                self.pos += 1;
                const is_array_table = self.pos < self.source.len and self.source[self.pos] == '[';
                if (is_array_table) self.pos += 1;
                
                const table_name = try self.parseTableName();
                current_table = table_name;
                
                // Ensure table exists
                if (!root.contains(table_name)) {
                    try root.put(try self.allocator.dupe(u8, table_name), TomlValue{ 
                        .table = std.StringHashMap(TomlValue).init(self.allocator) 
                    });
                }
            } else {
                // Key-value pair
                const key = try self.parseKey();
                self.skipWhitespace();
                
                if (self.pos >= self.source.len or self.source[self.pos] != '=') {
                    return error.InvalidSyntax;
                }
                self.pos += 1;
                self.skipWhitespace();
                
                const value = try self.parseValue();
                
                if (current_table) |table| {
                    if (root.get(table)) |table_value| {
                        switch (table_value) {
                            .table => |*t| {
                                try t.put(try self.allocator.dupe(u8, key), value);
                            },
                            else => return error.InvalidSyntax,
                        }
                    }
                } else {
                    try root.put(try self.allocator.dupe(u8, key), value);
                }
            }
        }
        
        return TomlValue{ .table = root };
    }
    
    fn parseTableName(self: *TomlParser) ![]const u8 {
        const start = self.pos;
        while (self.pos < self.source.len and self.source[self.pos] != ']') {
            self.pos += 1;
        }
        
        if (self.pos >= self.source.len) return error.InvalidSyntax;
        
        const name = self.source[start..self.pos];
        self.pos += 1; // Skip ]
        if (self.pos < self.source.len and self.source[self.pos] == ']') {
            self.pos += 1; // Skip second ] for array tables
        }
        
        return name;
    }
    
    fn parseKey(self: *TomlParser) ![]const u8 {
        const start = self.pos;
        while (self.pos < self.source.len) {
            const ch = self.source[self.pos];
            if (ch == '=' or ch == ' ' or ch == '\t' or ch == '\n' or ch == '\r') break;
            self.pos += 1;
        }
        
        if (start == self.pos) return error.InvalidKey;
        return self.source[start..self.pos];
    }
    
    fn parseValue(self: *TomlParser) !TomlValue {
        if (self.pos >= self.source.len) return error.InvalidValue;
        
        const ch = self.source[self.pos];
        
        if (ch == '"') {
            return TomlValue{ .string = try self.parseString() };
        } else if (ch == '[') {
            return TomlValue{ .array = try self.parseArray() };
        } else if (ch == '{') {
            return TomlValue{ .table = try self.parseInlineTable() };
        } else if (ch == 't' or ch == 'f') {
            return TomlValue{ .boolean = try self.parseBoolean() };
        } else if (std.ascii.isDigit(ch) or ch == '-' or ch == '+') {
            return self.parseNumber();
        } else {
            return error.InvalidValue;
        }
    }
    
    fn parseString(self: *TomlParser) ![]const u8 {
        self.pos += 1; // Skip opening quote
        const start = self.pos;
        
        while (self.pos < self.source.len and self.source[self.pos] != '"') {
            if (self.source[self.pos] == '\\') {
                self.pos += 2; // Skip escape sequence
            } else {
                self.pos += 1;
            }
        }
        
        if (self.pos >= self.source.len) return error.InvalidSyntax;
        
        const str = self.source[start..self.pos];
        self.pos += 1; // Skip closing quote
        
        return self.allocator.dupe(u8, str);
    }
    
    fn parseArray(self: *TomlParser) ![]TomlValue {
        self.pos += 1; // Skip [
        var items = std.ArrayList(TomlValue).init(self.allocator);
        errdefer items.deinit();
        
        while (self.pos < self.source.len) {
            self.skipWhitespaceAndComments();
            if (self.pos >= self.source.len) return error.InvalidSyntax;
            
            if (self.source[self.pos] == ']') {
                self.pos += 1;
                break;
            }
            
            try items.append(try self.parseValue());
            
            self.skipWhitespaceAndComments();
            if (self.pos < self.source.len and self.source[self.pos] == ',') {
                self.pos += 1;
            }
        }
        
        return items.toOwnedSlice();
    }
    
    fn parseInlineTable(self: *TomlParser) !std.StringHashMap(TomlValue) {
        self.pos += 1; // Skip {
        var table = std.StringHashMap(TomlValue).init(self.allocator);
        errdefer table.deinit();
        
        while (self.pos < self.source.len) {
            self.skipWhitespace();
            if (self.pos >= self.source.len) return error.InvalidSyntax;
            
            if (self.source[self.pos] == '}') {
                self.pos += 1;
                break;
            }
            
            const key = try self.parseKey();
            self.skipWhitespace();
            
            if (self.pos >= self.source.len or self.source[self.pos] != '=') {
                return error.InvalidSyntax;
            }
            self.pos += 1;
            self.skipWhitespace();
            
            const value = try self.parseValue();
            try table.put(try self.allocator.dupe(u8, key), value);
            
            self.skipWhitespace();
            if (self.pos < self.source.len and self.source[self.pos] == ',') {
                self.pos += 1;
            }
        }
        
        return table;
    }
    
    fn parseBoolean(self: *TomlParser) !bool {
        if (self.pos + 4 <= self.source.len and std.mem.eql(u8, self.source[self.pos..self.pos + 4], "true")) {
            self.pos += 4;
            return true;
        } else if (self.pos + 5 <= self.source.len and std.mem.eql(u8, self.source[self.pos..self.pos + 5], "false")) {
            self.pos += 5;
            return false;
        }
        return error.InvalidValue;
    }
    
    fn parseNumber(self: *TomlParser) !TomlValue {
        const start = self.pos;
        var has_dot = false;
        
        if (self.source[self.pos] == '-' or self.source[self.pos] == '+') {
            self.pos += 1;
        }
        
        while (self.pos < self.source.len) {
            const ch = self.source[self.pos];
            if (std.ascii.isDigit(ch)) {
                self.pos += 1;
            } else if (ch == '.' and !has_dot) {
                has_dot = true;
                self.pos += 1;
            } else {
                break;
            }
        }
        
        const num_str = self.source[start..self.pos];
        
        if (has_dot) {
            const val = try std.fmt.parseFloat(f64, num_str);
            return TomlValue{ .float = val };
        } else {
            const val = try std.fmt.parseInt(i64, num_str, 10);
            return TomlValue{ .integer = val };
        }
    }
    
    fn skipWhitespace(self: *TomlParser) void {
        while (self.pos < self.source.len) {
            const ch = self.source[self.pos];
            if (ch != ' ' and ch != '\t' and ch != '\r') break;
            self.pos += 1;
        }
    }
    
    fn skipWhitespaceAndComments(self: *TomlParser) void {
        while (self.pos < self.source.len) {
            const ch = self.source[self.pos];
            if (ch == ' ' or ch == '\t' or ch == '\r') {
                self.pos += 1;
            } else if (ch == '\n') {
                self.pos += 1;
            } else if (ch == '#') {
                // Skip comment line
                while (self.pos < self.source.len and self.source[self.pos] != '\n') {
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
    }
};

// Helper functions for easy access
pub fn getString(value: TomlValue, key: []const u8) ?[]const u8 {
    switch (value) {
        .table => |t| {
            if (t.get(key)) |v| {
                switch (v) {
                    .string => |s| return s,
                    else => return null,
                }
            }
        },
        else => {},
    }
    return null;
}

pub fn getInteger(value: TomlValue, key: []const u8) ?i64 {
    switch (value) {
        .table => |t| {
            if (t.get(key)) |v| {
                switch (v) {
                    .integer => |i| return i,
                    else => return null,
                }
            }
        },
        else => {},
    }
    return null;
}

pub fn getBoolean(value: TomlValue, key: []const u8) ?bool {
    switch (value) {
        .table => |t| {
            if (t.get(key)) |v| {
                switch (v) {
                    .boolean => |b| return b,
                    else => return null,
                }
            }
        },
        else => {},
    }
    return null;
}

pub fn getTable(value: TomlValue, key: []const u8) ?std.StringHashMap(TomlValue) {
    switch (value) {
        .table => |t| {
            if (t.get(key)) |v| {
                switch (v) {
                    .table => |subtable| return subtable,
                    else => return null,
                }
            }
        },
        else => {},
    }
    return null;
}