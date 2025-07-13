const std = @import("std");
const Package = @import("../core/package.zig").Package;
const TrustLevel = @import("../trust/trust.zig").TrustLevel;

pub const TuiError = error{
    TerminalError,
    InputError,
    RenderError,
};

pub const Tab = enum {
    search,
    installed,
    updates,
    trust,
    build,
    
    pub fn name(self: Tab) []const u8 {
        return switch (self) {
            .search => "Search",
            .installed => "Installed",
            .updates => "Updates",
            .trust => "Trust",
            .build => "Build",
        };
    }
};

pub const TuiState = struct {
    active_tab: Tab = .search,
    search_query: std.ArrayList(u8),
    search_results: []Package = &.{},
    selected_index: usize = 0,
    show_details: bool = false,
    scroll_offset: usize = 0,
    terminal_size: TerminalSize = .{ .width = 80, .height = 24 },
    
    pub fn init(allocator: std.mem.Allocator) TuiState {
        return .{
            .search_query = std.ArrayList(u8).init(allocator),
        };
    }
    
    pub fn deinit(self: *TuiState, allocator: std.mem.Allocator) void {
        self.search_query.deinit();
        if (self.search_results.len > 0) {
            allocator.free(self.search_results);
        }
    }
};

pub const TerminalSize = struct {
    width: u16,
    height: u16,
};

pub const Tui = struct {
    allocator: std.mem.Allocator,
    state: TuiState,
    stdin: std.fs.File,
    stdout: std.fs.File,
    original_termios: ?std.posix.termios = null,
    
    pub fn init(allocator: std.mem.Allocator) !*Tui {
        var self = try allocator.create(Tui);
        self.* = .{
            .allocator = allocator,
            .state = TuiState.init(allocator),
            .stdin = std.io.getStdIn(),
            .stdout = std.io.getStdOut(),
        };
        
        try self.enterRawMode();
        try self.getTerminalSize();
        
        return self;
    }
    
    pub fn deinit(self: *Tui) void {
        self.exitRawMode() catch {};
        self.state.deinit(self.allocator);
        self.allocator.destroy(self);
    }
    
    pub fn run(self: *Tui) !void {
        try self.clearScreen();
        try self.hideCursor();
        
        main_loop: while (true) {
            try self.render();
            
            const key = try self.readKey();
            
            switch (key) {
                .char => |c| switch (c) {
                    'q' => break :main_loop,
                    '\t' => self.nextTab(),
                    '\n', '\r' => try self.handleEnter(),
                    '\x7f', '\x08' => try self.handleBackspace(), // Backspace/Delete
                    ' ' => try self.handleSpace(),
                    'j' => self.moveDown(),
                    'k' => self.moveUp(),
                    'h' => self.moveLeft(),
                    'l' => self.moveRight(),
                    '/' => try self.startSearch(),
                    'd' => self.toggleDetails(),
                    'r' => try self.refresh(),
                    'i' => try self.installSelected(),
                    'u' => try self.uninstallSelected(),
                    't' => try self.showTrustInfo(),
                    else => {
                        if (self.state.active_tab == .search) {
                            try self.state.search_query.append(c);
                        }
                    },
                },
                .escape => try self.handleEscape(),
                .arrow_up => self.moveUp(),
                .arrow_down => self.moveDown(),
                .arrow_left => self.moveLeft(),
                .arrow_right => self.moveRight(),
                .page_up => self.pageUp(),
                .page_down => self.pageDown(),
                .home => self.moveToTop(),
                .end => self.moveToBottom(),
                .f => |n| try self.handleFunctionKey(n),
            }
        }
        
        try self.showCursor();
        try self.clearScreen();
    }
    
    pub fn setSearchResults(self: *Tui, results: []Package) void {
        if (self.state.search_results.len > 0) {
            self.allocator.free(self.state.search_results);
        }
        self.state.search_results = results;
        self.state.selected_index = 0;
        self.state.scroll_offset = 0;
    }
    
    fn render(self: *Tui) !void {
        try self.clearScreen();
        try self.moveCursor(1, 1);
        
        const writer = self.stdout.writer();
        
        // Render header
        try self.renderHeader(writer);
        
        // Render tabs
        try self.renderTabs(writer);
        
        // Render content based on active tab
        switch (self.state.active_tab) {
            .search => try self.renderSearchTab(writer),
            .installed => try self.renderInstalledTab(writer),
            .updates => try self.renderUpdatesTab(writer),
            .trust => try self.renderTrustTab(writer),
            .build => try self.renderBuildTab(writer),
        }
        
        // Render footer
        try self.renderFooter(writer);
        
        try writer.context.flush();
    }
    
    fn renderHeader(self: *Tui, writer: anytype) !void {
        const header = "Reaper - Unified AUR Helper & Build System";
        const padding = (self.state.terminal_size.width - header.len) / 2;
        
        try writer.print("\x1b[1;37m"); // Bold white
        try writer.writeByteNTimes(' ', padding);
        try writer.print("{s}", .{header});
        try writer.print("\x1b[0m\n"); // Reset
        
        // Separator line
        try writer.print("\x1b[2m"); // Dim
        try writer.writeByteNTimes('─', self.state.terminal_size.width);
        try writer.print("\x1b[0m\n"); // Reset
    }
    
    fn renderTabs(self: *Tui, writer: anytype) !void {
        const tabs = [_]Tab{ .search, .installed, .updates, .trust, .build };
        
        for (tabs, 0..) |tab, i| {
            if (tab == self.state.active_tab) {
                try writer.print("\x1b[1;33m[{s}]\x1b[0m ", .{tab.name()}); // Bold yellow
            } else {
                try writer.print("\x1b[2m {s} \x1b[0m ", .{tab.name()}); // Dim
            }
            
            if (i < tabs.len - 1) {
                try writer.print("│ ");
            }
        }
        try writer.print("\n\n");
    }
    
    fn renderSearchTab(self: *Tui, writer: anytype) !void {
        // Search input
        try writer.print("Search: \x1b[4m{s}\x1b[0m\n\n", .{self.state.search_query.items});
        
        if (self.state.search_results.len == 0) {
            try writer.print("No results. Type to search, press Enter to execute.\n");
            return;
        }
        
        // Results list
        const visible_height = self.state.terminal_size.height - 10; // Reserve space for header/footer
        const start_idx = self.state.scroll_offset;
        const end_idx = @min(start_idx + visible_height, self.state.search_results.len);
        
        for (self.state.search_results[start_idx..end_idx], start_idx..) |pkg, i| {
            const is_selected = i == self.state.selected_index;
            
            if (is_selected) {
                try writer.print("\x1b[1;44m"); // Bold blue background
            }
            
            // Trust level indicator
            const trust_level = TrustLevel.fromScore(pkg.trust_score);
            try writer.print("{s}●\x1b[0m ", .{trust_level.color()});
            
            if (is_selected) {
                try writer.print("\x1b[1;44m"); // Restore selection background
            }
            
            // Package info
            try writer.print("{s} {s}", .{ pkg.name, pkg.version });
            
            if (pkg.description.len > 0) {
                const max_desc_len = self.state.terminal_size.width - pkg.name.len - pkg.version.len - 10;
                const desc = if (pkg.description.len > max_desc_len) 
                    pkg.description[0..max_desc_len] 
                else 
                    pkg.description;
                try writer.print(" - {s}", .{desc});
            }
            
            if (is_selected) {
                try writer.print("\x1b[0m"); // Reset
            }
            
            try writer.print("\n");
            
            // Show detailed info if selected and details enabled
            if (is_selected and self.state.show_details) {
                try self.renderPackageDetails(writer, pkg);
            }
        }
        
        // Scroll indicator
        if (self.state.search_results.len > visible_height) {
            try writer.print("\n\x1b[2mShowing {}-{} of {} ({}%)\x1b[0m\n", .{
                start_idx + 1,
                end_idx,
                self.state.search_results.len,
                (end_idx * 100) / self.state.search_results.len,
            });
        }
    }
    
    fn renderPackageDetails(self: *Tui, writer: anytype, pkg: Package) !void {
        _ = self;
        try writer.print("\x1b[2m"); // Dim
        try writer.print("  Trust Score: {d:.1}/10 | Votes: {} | Popularity: {d:.1}\n", .{
            pkg.trust_score, pkg.votes, pkg.popularity
        });
        try writer.print("  Maintainer: {s} | Type: {s}\n", .{
            pkg.maintainer, @tagName(pkg.package_type)
        });
        if (pkg.url.len > 0) {
            try writer.print("  URL: {s}\n", .{pkg.url});
        }
        try writer.print("\x1b[0m"); // Reset
    }
    
    fn renderInstalledTab(self: *Tui, writer: anytype) !void {
        _ = self;
        try writer.print("Installed packages will be shown here.\n");
        try writer.print("Press 'r' to refresh the list.\n");
    }
    
    fn renderUpdatesTab(self: *Tui, writer: anytype) !void {
        _ = self;
        try writer.print("Available updates will be shown here.\n");
        try writer.print("Press 'r' to check for updates.\n");
    }
    
    fn renderTrustTab(self: *Tui, writer: anytype) !void {
        _ = self;
        try writer.print("Trust analysis and security information.\n");
        try writer.print("Press 't' on a package to view detailed trust info.\n");
    }
    
    fn renderBuildTab(self: *Tui, writer: anytype) !void {
        _ = self;
        try writer.print("Build system for local projects.\n");
        try writer.print("Auto-detects project type and builds accordingly.\n");
    }
    
    fn renderFooter(self: *Tui, writer: anytype) !void {
        _ = self;
        try writer.print("\n\x1b[2m"); // Dim
        try writer.print("q:quit │ tab:switch │ j/k:nav │ enter:action │ d:details │ i:install │ u:remove │ /:search");
        try writer.print("\x1b[0m"); // Reset
    }
    
    // Input handling
    fn readKey(self: *Tui) !Key {
        var buf: [8]u8 = undefined;
        const n = try self.stdin.read(&buf);
        
        if (n == 0) return Key{ .char = 0 };
        
        // Handle escape sequences
        if (buf[0] == '\x1b' and n >= 3) {
            if (buf[1] == '[') {
                switch (buf[2]) {
                    'A' => return .arrow_up,
                    'B' => return .arrow_down,
                    'C' => return .arrow_right,
                    'D' => return .arrow_left,
                    'H' => return .home,
                    'F' => return .end,
                    '5' => if (n >= 4 and buf[3] == '~') return .page_up,
                    '6' => if (n >= 4 and buf[3] == '~') return .page_down,
                    else => {},
                }
            }
            return .escape;
        }
        
        // Function keys
        if (buf[0] >= '\x01' and buf[0] <= '\x0c') {
            return Key{ .f = buf[0] };
        }
        
        return Key{ .char = buf[0] };
    }
    
    // Navigation
    fn nextTab(self: *Tui) void {
        const tabs = [_]Tab{ .search, .installed, .updates, .trust, .build };
        for (tabs, 0..) |tab, i| {
            if (tab == self.state.active_tab) {
                self.state.active_tab = tabs[(i + 1) % tabs.len];
                break;
            }
        }
    }
    
    fn moveUp(self: *Tui) void {
        if (self.state.selected_index > 0) {
            self.state.selected_index -= 1;
            if (self.state.selected_index < self.state.scroll_offset) {
                self.state.scroll_offset = self.state.selected_index;
            }
        }
    }
    
    fn moveDown(self: *Tui) void {
        if (self.state.selected_index < self.state.search_results.len - 1) {
            self.state.selected_index += 1;
            const visible_height = self.state.terminal_size.height - 10;
            if (self.state.selected_index >= self.state.scroll_offset + visible_height) {
                self.state.scroll_offset = self.state.selected_index - visible_height + 1;
            }
        }
    }
    
    fn moveLeft(self: *Tui) void {
        // Implementation for horizontal movement if needed
    }
    
    fn moveRight(self: *Tui) void {
        // Implementation for horizontal movement if needed
    }
    
    fn pageUp(self: *Tui) void {
        const page_size = self.state.terminal_size.height - 10;
        if (self.state.selected_index >= page_size) {
            self.state.selected_index -= page_size;
        } else {
            self.state.selected_index = 0;
        }
        self.state.scroll_offset = self.state.selected_index;
    }
    
    fn pageDown(self: *Tui) void {
        const page_size = self.state.terminal_size.height - 10;
        self.state.selected_index = @min(
            self.state.selected_index + page_size,
            self.state.search_results.len - 1
        );
        const visible_height = self.state.terminal_size.height - 10;
        if (self.state.selected_index >= self.state.scroll_offset + visible_height) {
            self.state.scroll_offset = self.state.selected_index - visible_height + 1;
        }
    }
    
    fn moveToTop(self: *Tui) void {
        self.state.selected_index = 0;
        self.state.scroll_offset = 0;
    }
    
    fn moveToBottom(self: *Tui) void {
        if (self.state.search_results.len > 0) {
            self.state.selected_index = self.state.search_results.len - 1;
            const visible_height = self.state.terminal_size.height - 10;
            self.state.scroll_offset = if (self.state.search_results.len > visible_height)
                self.state.search_results.len - visible_height
            else
                0;
        }
    }
    
    // Action handlers
    fn handleEnter(self: *Tui) !void {
        _ = self;
        // Implementation for enter action based on current context
    }
    
    fn handleBackspace(self: *Tui) !void {
        if (self.state.active_tab == .search and self.state.search_query.items.len > 0) {
            _ = self.state.search_query.pop();
        }
    }
    
    fn handleSpace(self: *Tui) !void {
        if (self.state.active_tab == .search) {
            try self.state.search_query.append(' ');
        }
    }
    
    fn handleEscape(self: *Tui) !void {
        self.state.show_details = false;
    }
    
    fn handleFunctionKey(self: *Tui, key: u8) !void {
        _ = self;
        _ = key;
        // Implementation for function keys
    }
    
    fn startSearch(self: *Tui) !void {
        self.state.active_tab = .search;
        self.state.search_query.clearRetainingCapacity();
    }
    
    fn toggleDetails(self: *Tui) void {
        self.state.show_details = !self.state.show_details;
    }
    
    fn refresh(self: *Tui) !void {
        _ = self;
        // Implementation for refresh action
    }
    
    fn installSelected(self: *Tui) !void {
        _ = self;
        // Implementation for install action
    }
    
    fn uninstallSelected(self: *Tui) !void {
        _ = self;
        // Implementation for uninstall action
    }
    
    fn showTrustInfo(self: *Tui) !void {
        _ = self;
        // Implementation for showing trust information
    }
    
    // Terminal control
    fn enterRawMode(self: *Tui) !void {
        const fd = self.stdin.handle;
        self.original_termios = try std.posix.tcgetattr(fd);
        
        var raw = self.original_termios.?;
        raw.iflag.BRKINT = false;
        raw.iflag.ICRNL = false;
        raw.iflag.INPCK = false;
        raw.iflag.ISTRIP = false;
        raw.iflag.IXON = false;
        raw.oflag.OPOST = false;
        raw.cflag.CS8 = true;
        raw.lflag.ECHO = false;
        raw.lflag.ICANON = false;
        raw.lflag.IEXTEN = false;
        raw.lflag.ISIG = false;
        raw.cc[@intFromEnum(std.posix.V.MIN)] = 1;
        raw.cc[@intFromEnum(std.posix.V.TIME)] = 0;
        
        try std.posix.tcsetattr(fd, .FLUSH, raw);
    }
    
    fn exitRawMode(self: *Tui) !void {
        if (self.original_termios) |termios| {
            try std.posix.tcsetattr(self.stdin.handle, .FLUSH, termios);
        }
    }
    
    fn getTerminalSize(self: *Tui) !void {
        var ws: std.posix.winsize = undefined;
        _ = std.posix.system.ioctl(self.stdout.handle, std.posix.T.IOCGWINSZ, @intFromPtr(&ws));
        
        self.state.terminal_size.width = ws.ws_col;
        self.state.terminal_size.height = ws.ws_row;
    }
    
    fn clearScreen(self: *Tui) !void {
        try self.stdout.writeAll("\x1b[2J");
    }
    
    fn moveCursor(self: *Tui, row: u16, col: u16) !void {
        try self.stdout.writer().print("\x1b[{};{}H", .{ row, col });
    }
    
    fn hideCursor(self: *Tui) !void {
        try self.stdout.writeAll("\x1b[?25l");
    }
    
    fn showCursor(self: *Tui) !void {
        try self.stdout.writeAll("\x1b[?25h");
    }
};

const Key = union(enum) {
    char: u8,
    escape,
    arrow_up,
    arrow_down,
    arrow_left,
    arrow_right,
    page_up,
    page_down,
    home,
    end,
    f: u8,
};