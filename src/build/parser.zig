const std = @import("std");

pub const Parser = struct {
    allocator: std.mem.Allocator,
    
    pub fn init(allocator: std.mem.Allocator) !*Parser {
        var self = try allocator.create(Parser);
        self.* = .{
            .allocator = allocator,
        };
        return self;
    }
    
    pub fn deinit(self: *Parser) void {
        self.allocator.destroy(self);
    }
    
    pub fn parsePkgbuild(self: *Parser, path: []const u8) !Pkgbuild {
        const file = try std.fs.openFileAbsolute(path, .{});
        defer file.close();
        
        const contents = try file.readToEndAlloc(self.allocator, 1024 * 1024);
        defer self.allocator.free(contents);
        
        var pkgbuild = Pkgbuild{
            .allocator = self.allocator,
            .pkgname = "",
            .pkgver = "",
            .pkgrel = "1",
            .pkgdesc = "",
            .arch = &.{},
            .url = "",
            .license = &.{},
            .depends = &.{},
            .makedepends = &.{},
            .optdepends = &.{},
            .provides = &.{},
            .conflicts = &.{},
            .replaces = &.{},
            .source = &.{},
            .sha256sums = &.{},
            .prepare_fn = null,
            .build_fn = null,
            .package_fn = "",
        };
        
        var lines = std.mem.tokenize(u8, contents, "\n");
        var in_function = false;
        var current_function: ?*[]const u8 = null;
        var function_content = std.ArrayList(u8).init(self.allocator);
        defer function_content.deinit();
        
        while (lines.next()) |line| {
            const trimmed = std.mem.trim(u8, line, " \t");
            
            // Skip comments and empty lines
            if (trimmed.len == 0 or trimmed[0] == '#') continue;
            
            // Check for function definitions
            if (std.mem.endsWith(u8, trimmed, "() {")) {
                in_function = true;
                function_content.clearRetainingCapacity();
                
                if (std.mem.startsWith(u8, trimmed, "prepare")) {
                    current_function = &pkgbuild.prepare_fn;
                } else if (std.mem.startsWith(u8, trimmed, "build")) {
                    current_function = &pkgbuild.build_fn;
                } else if (std.mem.startsWith(u8, trimmed, "package")) {
                    current_function = &pkgbuild.package_fn;
                }
                continue;
            }
            
            // Handle function content
            if (in_function) {
                if (std.mem.eql(u8, trimmed, "}")) {
                    in_function = false;
                    if (current_function) |func_ptr| {
                        func_ptr.* = try function_content.toOwnedSlice();
                    }
                    current_function = null;
                } else {
                    try function_content.appendSlice(line);
                    try function_content.append('\n');
                }
                continue;
            }
            
            // Parse variable assignments
            if (std.mem.indexOf(u8, trimmed, "=")) |eq_pos| {
                const var_name = std.mem.trim(u8, trimmed[0..eq_pos], " \t");
                const var_value = std.mem.trim(u8, trimmed[eq_pos + 1..], " \t\"'");
                
                if (std.mem.eql(u8, var_name, "pkgname")) {
                    pkgbuild.pkgname = try self.allocator.dupe(u8, var_value);
                } else if (std.mem.eql(u8, var_name, "pkgver")) {
                    pkgbuild.pkgver = try self.allocator.dupe(u8, var_value);
                } else if (std.mem.eql(u8, var_name, "pkgrel")) {
                    pkgbuild.pkgrel = try self.allocator.dupe(u8, var_value);
                } else if (std.mem.eql(u8, var_name, "pkgdesc")) {
                    pkgbuild.pkgdesc = try self.allocator.dupe(u8, var_value);
                } else if (std.mem.eql(u8, var_name, "url")) {
                    pkgbuild.url = try self.allocator.dupe(u8, var_value);
                } else if (std.mem.eql(u8, var_name, "arch")) {
                    pkgbuild.arch = try self.parseArray(var_value);
                } else if (std.mem.eql(u8, var_name, "license")) {
                    pkgbuild.license = try self.parseArray(var_value);
                } else if (std.mem.eql(u8, var_name, "depends")) {
                    pkgbuild.depends = try self.parseArray(var_value);
                } else if (std.mem.eql(u8, var_name, "makedepends")) {
                    pkgbuild.makedepends = try self.parseArray(var_value);
                } else if (std.mem.eql(u8, var_name, "source")) {
                    pkgbuild.source = try self.parseArray(var_value);
                } else if (std.mem.eql(u8, var_name, "sha256sums")) {
                    pkgbuild.sha256sums = try self.parseArray(var_value);
                }
            }
        }
        
        return pkgbuild;
    }
    
    pub fn parseZmkToml(self: *Parser, path: []const u8) !ZmkToml {
        const file = try std.fs.openFileAbsolute(path, .{});
        defer file.close();
        
        const contents = try file.readToEndAlloc(self.allocator, 1024 * 1024);
        defer self.allocator.free(contents);
        
        const toml = @import("../utils/toml.zig");
        var parser = toml.TomlParser.init(self.allocator, contents);
        var root = try parser.parse();
        defer root.deinit(self.allocator);
        
        var zmk = ZmkToml{
            .allocator = self.allocator,
            .package = .{
                .name = "",
                .version = "",
                .description = "",
                .authors = &.{},
                .license = "",
            },
            .dependencies = &.{},
            .build_dependencies = &.{},
            .sources = &.{},
            .build = .{
                .type = .auto_detect,
                .jobs = null,
                .flags = &.{},
            },
        };
        
        // Parse package section
        if (toml.getTable(root, "package")) |pkg| {
            if (toml.getString(toml.TomlValue{ .table = pkg }, "name")) |name| {
                zmk.package.name = try self.allocator.dupe(u8, name);
            }
            if (toml.getString(toml.TomlValue{ .table = pkg }, "version")) |version| {
                zmk.package.version = try self.allocator.dupe(u8, version);
            }
            if (toml.getString(toml.TomlValue{ .table = pkg }, "description")) |desc| {
                zmk.package.description = try self.allocator.dupe(u8, desc);
            }
            if (toml.getString(toml.TomlValue{ .table = pkg }, "license")) |license| {
                zmk.package.license = try self.allocator.dupe(u8, license);
            }
        }
        
        // Parse dependencies
        if (toml.getTable(root, "dependencies")) |deps| {
            zmk.dependencies = try self.parseDependencies(deps);
        }
        
        // Parse build dependencies
        if (toml.getTable(root, "build-dependencies")) |build_deps| {
            zmk.build_dependencies = try self.parseDependencies(build_deps);
        }
        
        // Parse sources
        if (root.table.get("sources")) |sources_value| {
            switch (sources_value) {
                .array => |arr| {
                    var sources = std.ArrayList(Source).init(self.allocator);
                    defer sources.deinit();
                    
                    for (arr) |item| {
                        switch (item) {
                            .table => |t| {
                                var source = Source{
                                    .url = "",
                                    .checksum = null,
                                    .extract = true,
                                };
                                
                                if (toml.getString(toml.TomlValue{ .table = t }, "url")) |url| {
                                    source.url = try self.allocator.dupe(u8, url);
                                }
                                if (toml.getString(toml.TomlValue{ .table = t }, "checksum")) |checksum| {
                                    source.checksum = try self.allocator.dupe(u8, checksum);
                                }
                                if (toml.getBoolean(toml.TomlValue{ .table = t }, "extract")) |extract| {
                                    source.extract = extract;
                                }
                                
                                try sources.append(source);
                            },
                            else => {},
                        }
                    }
                    
                    zmk.sources = try sources.toOwnedSlice();
                },
                else => {},
            }
        }
        
        // Parse build section
        if (toml.getTable(root, "build")) |build| {
            if (toml.getString(toml.TomlValue{ .table = build }, "type")) |build_type| {
                zmk.build.type = std.meta.stringToEnum(@import("builder.zig").BuildType, build_type) orelse .auto_detect;
            }
            if (toml.getInteger(toml.TomlValue{ .table = build }, "jobs")) |jobs| {
                zmk.build.jobs = @intCast(jobs);
            }
        }
        
        return zmk;
    }
    
    fn parseDependencies(self: *Parser, deps_table: std.StringHashMap(toml.TomlValue)) ![]Dependency {
        var deps = std.ArrayList(Dependency).init(self.allocator);
        defer deps.deinit();
        
        var iter = deps_table.iterator();
        while (iter.next()) |entry| {
            var dep = Dependency{
                .name = try self.allocator.dupe(u8, entry.key_ptr.*),
                .version = null,
                .features = &.{},
            };
            
            switch (entry.value_ptr.*) {
                .string => |ver| {
                    dep.version = try self.allocator.dupe(u8, ver);
                },
                .table => |t| {
                    if (toml.getString(toml.TomlValue{ .table = t }, "version")) |ver| {
                        dep.version = try self.allocator.dupe(u8, ver);
                    }
                },
                else => {},
            }
            
            try deps.append(dep);
        }
        
        return deps.toOwnedSlice();
    }
    
    fn parseArray(self: *Parser, value: []const u8) ![][]const u8 {
        var items = std.ArrayList([]const u8).init(self.allocator);
        defer items.deinit();
        
        const cleaned = std.mem.trim(u8, value, "()");
        var iter = std.mem.tokenize(u8, cleaned, " \t'\"");
        
        while (iter.next()) |item| {
            try items.append(try self.allocator.dupe(u8, item));
        }
        
        return items.toOwnedSlice();
    }
};

pub const Pkgbuild = struct {
    allocator: std.mem.Allocator,
    
    // Package metadata
    pkgname: []const u8,
    pkgver: []const u8,
    pkgrel: []const u8,
    pkgdesc: []const u8,
    arch: [][]const u8,
    url: []const u8,
    license: [][]const u8,
    
    // Dependencies
    depends: [][]const u8,
    makedepends: [][]const u8,
    optdepends: [][]const u8,
    provides: [][]const u8,
    conflicts: [][]const u8,
    replaces: [][]const u8,
    
    // Sources
    source: [][]const u8,
    sha256sums: [][]const u8,
    
    // Build functions
    prepare_fn: ?[]const u8,
    build_fn: ?[]const u8,
    package_fn: []const u8,
    
    pub fn deinit(self: *Pkgbuild) void {
        self.allocator.free(self.pkgname);
        self.allocator.free(self.pkgver);
        self.allocator.free(self.pkgrel);
        self.allocator.free(self.pkgdesc);
        self.allocator.free(self.url);
        
        for (self.arch) |item| self.allocator.free(item);
        self.allocator.free(self.arch);
        
        for (self.license) |item| self.allocator.free(item);
        self.allocator.free(self.license);
        
        for (self.depends) |item| self.allocator.free(item);
        self.allocator.free(self.depends);
        
        for (self.makedepends) |item| self.allocator.free(item);
        self.allocator.free(self.makedepends);
        
        for (self.source) |item| self.allocator.free(item);
        self.allocator.free(self.source);
        
        for (self.sha256sums) |item| self.allocator.free(item);
        self.allocator.free(self.sha256sums);
        
        if (self.prepare_fn) |fn| self.allocator.free(fn);
        if (self.build_fn) |fn| self.allocator.free(fn);
        self.allocator.free(self.package_fn);
    }
};

pub const ZmkToml = struct {
    allocator: std.mem.Allocator,
    
    package: struct {
        name: []const u8,
        version: []const u8,
        description: []const u8,
        authors: [][]const u8,
        license: []const u8,
    },
    
    dependencies: []Dependency,
    build_dependencies: []Dependency,
    sources: []Source,
    
    build: struct {
        type: @import("builder.zig").BuildType,
        jobs: ?u32,
        flags: [][]const u8,
    },
    
    pub fn deinit(self: *ZmkToml) void {
        self.allocator.free(self.package.name);
        self.allocator.free(self.package.version);
        self.allocator.free(self.package.description);
        self.allocator.free(self.package.license);
        
        for (self.package.authors) |author| {
            self.allocator.free(author);
        }
        self.allocator.free(self.package.authors);
    }
};

pub const Dependency = struct {
    name: []const u8,
    version: ?[]const u8,
    features: [][]const u8,
};

pub const Source = struct {
    url: []const u8,
    checksum: ?[]const u8,
    extract: bool = true,
};