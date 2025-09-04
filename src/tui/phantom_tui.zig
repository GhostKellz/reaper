const std = @import("std");
const phantom = @import("phantom");
const zsync = @import("zsync");
const HttpClient = @import("../network/http_client.zig").HttpClient;

const Package = @import("../core/package.zig").Package;
const TrustLevel = @import("../trust/trust.zig").TrustLevel;
const AurBackend = @import("../backends/aur.zig").AurBackend;

pub const PhantomTuiError = error{
    InitializationFailed,
    RenderError,
    NetworkError,
    AsyncError,
};

pub const AppMode = enum {
    package_browser,
    dependency_viewer,
    search_results,
    install_progress,
    repository_explorer,
};

pub const PackageSource = enum {
    aur,
    pacman,
    chaotic_aur,
    custom,

    pub fn displayName(self: PackageSource) []const u8 {
        return switch (self) {
            .aur => "AUR",
            .pacman => "Pacman",
            .chaotic_aur => "Chaotic AUR",
            .custom => "Custom",
        };
    }

    pub fn color(self: PackageSource) []const u8 {
        return switch (self) {
            .aur => "\x1b[1;34m", // Blue
            .pacman => "\x1b[1;32m", // Green
            .chaotic_aur => "\x1b[1;35m", // Magenta
            .custom => "\x1b[1;33m", // Yellow
        };
    }
};

pub const PackageEntry = struct {
    package: Package,
    source: PackageSource,
    dependencies: []PackageEntry = &.{},
    install_status: InstallStatus = .not_installed,

    pub const InstallStatus = enum {
        not_installed,
        installed,
        update_available,
        installing,
        failed,
    };
};

pub const PhantomTui = struct {
    allocator: std.mem.Allocator,
    runtime: *zsync.Runtime,
    http_client: *HttpClient,

    // Phantom app state
    app: phantom.App,
    task_monitor: *phantom.widgets.TaskMonitor,
    package_list: *phantom.widgets.List,
    progress_bar: *phantom.widgets.ProgressBar,

    // Application state
    mode: AppMode = .package_browser,
    packages: std.ArrayList(PackageEntry),
    selected_index: usize = 0,
    search_query: std.ArrayList(u8),
    repositories: std.ArrayList(Repository),
    background_tasks: std.ArrayList(BackgroundTask),

    // Async state
    search_task: ?u32 = null,
    update_check_task: ?u32 = null,
    install_tasks: std.ArrayList(u32),

    const Repository = struct {
        name: []const u8,
        url: []const u8,
        source: PackageSource,
        enabled: bool = true,
    };

    const BackgroundTask = struct {
        id: []const u8,
        description: []const u8,
        progress: f32 = 0.0,
        status: TaskStatus = .running,

        const TaskStatus = enum {
            pending,
            running,
            completed,
            failed,
        };
    };

    pub fn init(
        allocator: std.mem.Allocator,
        runtime: *zsync.Runtime,
        http_client: *HttpClient,
    ) !*PhantomTui {
        var self = try allocator.create(PhantomTui);
        errdefer allocator.destroy(self);

        // Initialize phantom app
        const app_config = phantom.AppConfig{
            .title = "Reaper - AUR Helper & Package Browser",
            .tick_rate_ms = 50,
        };

        self.app = try phantom.App.init(allocator, app_config);
        errdefer self.app.deinit();

        // Initialize widgets
        self.task_monitor = try phantom.widgets.TaskMonitor.init(allocator);

        self.package_list = try phantom.widgets.List.init(allocator);

        self.progress_bar = try phantom.widgets.ProgressBar.init(allocator);

        // Add widgets to app
        try self.app.addWidget(&self.task_monitor.widget);
        try self.app.addWidget(&self.package_list.widget);
        try self.app.addWidget(&self.progress_bar.widget);

        self.* = PhantomTui{
            .allocator = allocator,
            .runtime = runtime,
            .http_client = http_client,
            .app = self.app,
            .task_monitor = self.task_monitor,
            .package_list = self.package_list,
            .progress_bar = self.progress_bar,
            .packages = std.ArrayList(PackageEntry){},
            .search_query = std.ArrayList(u8){},
            .repositories = std.ArrayList(Repository){},
            .background_tasks = std.ArrayList(BackgroundTask){},
            .install_tasks = std.ArrayList(u32){},
        };

        // Initialize default repositories
        try self.initializeRepositories();

        // Start background update checker
        try self.startBackgroundUpdateChecker();

        return self;
    }

    pub fn deinit(self: *PhantomTui) void {
        // Cancel all async tasks
        if (self.search_task) |task_id| {
            // Note: zsync v0.3.2 task cancellation would be handled by the runtime
            _ = task_id;
        }
        if (self.update_check_task) |task_id| {
            // Note: zsync v0.3.2 task cancellation would be handled by the runtime
            _ = task_id;
        }
        for (self.install_tasks.items) |task_id| {
            // Note: zsync v0.3.2 task cancellation would be handled by the runtime
            _ = task_id;
        }

        // Cleanup collections
        self.packages.deinit(self.allocator);
        self.search_query.deinit(self.allocator);
        self.repositories.deinit(self.allocator);
        self.background_tasks.deinit(self.allocator);
        self.install_tasks.deinit(self.allocator);

        // Cleanup phantom app
        self.app.deinit();

        self.allocator.destroy(self);
    }

    pub fn run(self: *PhantomTui) !void {
        // Set up event handlers
        // Note: Event handling would be implemented in the main run loop

        // Initial package loading
        try self.loadPackagesAsync();

        // Run the phantom app
        try self.app.run();
    }

    fn initializeRepositories(self: *PhantomTui) !void {
        // Add default repositories
        try self.repositories.append(self.allocator, .{
            .name = "AUR",
            .url = "https://aur.archlinux.org/rpc",
            .source = .aur,
        });

        try self.repositories.append(self.allocator, .{
            .name = "Chaotic AUR",
            .url = "https://aur.chaotic.cx/rpc",
            .source = .chaotic_aur,
        });

        // Parse pacman.conf for additional repositories
        try self.parsePacmanConf();
    }

    fn parsePacmanConf(self: *PhantomTui) !void {
        const pacman_conf_path = "/etc/pacman.conf";

        // Read pacman.conf file
        const file = try std.fs.openFileAbsolute(pacman_conf_path, .{});
        defer file.close();
        const content = try file.readToEndAlloc(self.allocator, 1024 * 1024);
        defer self.allocator.free(content);

        var lines = std.mem.splitScalar(u8, content, '\n');
        while (lines.next()) |line| {
            const trimmed = std.mem.trim(u8, line, " \t");
            if (trimmed.len == 0 or trimmed[0] == '#') continue;

            if (std.mem.startsWith(u8, trimmed, "[") and std.mem.endsWith(u8, trimmed, "]")) {
                const repo_name = trimmed[1 .. trimmed.len - 1];
                if (!std.mem.eql(u8, repo_name, "options")) {
                    try self.repositories.append(self.allocator, .{
                        .name = try self.allocator.dupe(u8, repo_name),
                        .url = "", // Will be populated from server URLs
                        .source = .pacman,
                    });
                }
            }
        }
    }

    fn loadPackagesAsync(self: *PhantomTui) !void {
        // Create background task
        const task_id = try std.fmt.allocPrint(self.allocator, "load_packages_{d}", .{std.time.timestamp()});
        try self.addBackgroundTask(task_id, "Loading packages from repositories...");

        // Spawn async task to load packages from all repositories
        // TODO: Implement with new zsync Task API - temporarily complete task immediately
        self.completeBackgroundTask(task_id) catch {};
        
        /*
        // const load_task = try zsync.Task.init(self.allocator, struct {
            fn load(tui: *PhantomTui, bg_task_id: []const u8) !void {
                defer tui.completeBackgroundTask(bg_task_id) catch {};

                for (tui.repositories.items, 0..) |repo, i| {
                    const progress = @as(f32, @floatFromInt(i)) / @as(f32, @floatFromInt(tui.repositories.items.len));
                    try tui.updateBackgroundTaskProgress(bg_task_id, progress);

                    switch (repo.source) {
                        .aur, .chaotic_aur => try tui.loadAurPackages(repo),
                        .pacman => try tui.loadPacmanPackages(repo),
                        .custom => {},
                    }
                }

                // Update UI
                try tui.refreshPackageList();
            }
        }.load, .{ self, task_id }, .normal);

        // Don't await - let it run in background
        _ = load_task;
        */
    }

    fn loadAurPackages(self: *PhantomTui, repo: Repository) !void {
        // Use our HTTP client for AUR requests
        const url = try std.fmt.allocPrint(self.allocator, "{s}?type=search&arg=", .{repo.url});
        defer self.allocator.free(url);

        var response = try self.http_client.get(url);
        defer response.deinit();

        if (!response.isSuccess()) {
            std.log.err("Failed to fetch from {s}: HTTP {}", .{ repo.name, response.status_code });
            return;
        }

        // Parse JSON response and convert to PackageEntry
        // This would be implemented based on AUR API response format
        const json_data = try std.json.parseFromSlice(std.json.Value, self.allocator, response.body, .{});
        defer json_data.deinit();

        // Convert JSON to packages and add to list
        // Implementation would parse AUR API response format
    }

    fn loadPacmanPackages(self: *PhantomTui, repo: Repository) !void {
        // Use system commands to query pacman database
        const cmd = try std.fmt.allocPrint(self.allocator, "pacman -Sl {s}", .{repo.name});
        defer self.allocator.free(cmd);

        // Use zsync subprocess instead of manual threading
        const result = try self.runtime.spawnSubprocess(.{
            .argv = &.{ "sh", "-c", cmd },
            .stdout_behavior = .Pipe,
            .stderr_behavior = .Pipe,
        });

        const output = try result.stdout.readToEndAlloc(self.allocator, 1024 * 1024); // 1MB limit
        defer self.allocator.free(output);

        // Parse pacman output and convert to PackageEntry
        var lines = std.mem.split(u8, output, "\n");
        while (lines.next()) |line| {
            if (line.len == 0) continue;

            var parts = std.mem.split(u8, line, " ");
            const repo_name = parts.next() orelse continue;
            const package_name = parts.next() orelse continue;
            const version = parts.next() orelse continue;

            const package = Package{
                .name = try self.allocator.dupe(u8, package_name),
                .version = try self.allocator.dupe(u8, version),
                .description = "",
                .url = "",
                .maintainer = repo_name,
                .package_type = .official,
                .trust_score = 10.0, // Official packages are fully trusted
                .votes = 0,
                .popularity = 0.0,
                .dependencies = &.{},
                .optional_dependencies = &.{},
                .provides = &.{},
                .conflicts = &.{},
            };

            try self.packages.append(self.allocator, .{
                .package = package,
                .source = .pacman,
                .install_status = .not_installed, // Would check actual status
            });
        }
    }

    fn searchPackagesAsync(self: *PhantomTui, query: []const u8) !void {
        // Cancel existing search task
        if (self.search_task) |task| {
            task.cancel();
        }

        const task_id = try std.fmt.allocPrint(self.allocator, "search_{d}", .{std.time.timestamp()});
        try self.addBackgroundTask(task_id, try std.fmt.allocPrint(self.allocator, "Searching for '{s}'...", .{query}));

        self.search_task = try self.runtime.spawn(struct {
            fn search(tui: *PhantomTui, search_query: []const u8, bg_task_id: []const u8) !void {
                defer tui.completeBackgroundTask(bg_task_id) catch {};

                // Clear current results
                tui.packages.clearRetainingCapacity();

                // Search each repository
                for (tui.repositories.items, 0..) |repo, i| {
                    if (!repo.enabled) continue;

                    const progress = @as(f32, @floatFromInt(i)) / @as(f32, @floatFromInt(tui.repositories.items.len));
                    try tui.updateBackgroundTaskProgress(bg_task_id, progress);

                    switch (repo.source) {
                        .aur, .chaotic_aur => try tui.searchAurPackages(repo, search_query),
                        .pacman => try tui.searchPacmanPackages(repo, search_query),
                        .custom => {},
                    }
                }

                // Update UI
                try tui.refreshPackageList();
                tui.mode = .search_results;
            }
        }.search, .{ self, query, task_id }, .normal);
    }

    fn searchAurPackages(self: *PhantomTui, repo: Repository, query: []const u8) !void {
        const url = try std.fmt.allocPrint(self.allocator, "{s}?type=search&arg={s}", .{ repo.url, query });
        defer self.allocator.free(url);

        var response = try self.http_client.get(url);
        defer response.deinit();

        if (!response.isSuccess()) return;

        // Parse and add results to packages list
        // Implementation would handle AUR API JSON response
    }

    fn searchPacmanPackages(self: *PhantomTui, repo: Repository, query: []const u8) !void {
        const cmd = try std.fmt.allocPrint(self.allocator, "pacman -Ss --repo {s} {s}", .{ repo.name, query });
        defer self.allocator.free(cmd);

        const result = try self.runtime.spawnSubprocess(.{
            .argv = &.{ "sh", "-c", cmd },
            .stdout_behavior = .Pipe,
            .stderr_behavior = .Pipe,
        });

        const output = try result.stdout.readToEndAlloc(self.allocator, 1024 * 1024);
        defer self.allocator.free(output);

        // Parse and add results
        // Implementation would handle pacman search output format
    }

    fn startBackgroundUpdateChecker(self: *PhantomTui) !void {
        const task_id = "background_update_check";
        try self.addBackgroundTask(task_id, "Checking for updates...");

        // Temporarily disable background update checker until we implement proper async API
        self.completeBackgroundTask(task_id) catch {};
        
        // TODO: Implement with new zsync Task API
        // self.update_check_task = try zsync.Task.init(self.allocator, struct {
        //     fn checkUpdates(tui: *PhantomTui, bg_task_id: []const u8) !void {
        //         defer tui.completeBackgroundTask(bg_task_id) catch {};
        //         // Implementation with new zsync API
        //     }
        // }.checkUpdates, .{ self, "background_update_check" });
    }

    fn handleKeyEvent(self: *PhantomTui, key: phantom.Key) !void {
        switch (key) {
            .char => |c| switch (c) {
                'q' => try self.app.quit(),
                '/' => try self.startSearch(),
                's' => try self.searchPackagesAsync(self.search_query.items),
                'i' => try self.installSelectedPackage(),
                'u' => try self.uninstallSelectedPackage(),
                'r' => try self.loadPackagesAsync(),
                'd' => try self.showDependencies(),
                'b' => self.mode = .package_browser,
                else => {
                    // Add to search query if in search mode
                    if (self.mode == .search_results) {
                        try self.search_query.append(self.allocator, c);
                    }
                },
            },
            .escape => {
                self.mode = .package_browser;
                self.search_query.clearRetainingCapacity();
            },
            .enter => try self.handleEnterKey(),
            .backspace => {
                if (self.search_query.items.len > 0) {
                    _ = self.search_query.pop();
                }
            },
            .tab => try self.switchMode(),
            else => {},
        }
    }

    fn handleUpdate(self: *PhantomTui, dt: f32) !void {
        // Update background task progress in UI
        try self.updateTaskMonitor();

        // Update package list if needed
        if (self.packages.items.len > 0) {
            try self.refreshPackageList();
        }

        _ = dt;
    }

    fn installSelectedPackage(self: *PhantomTui) !void {
        if (self.selected_index >= self.packages.items.len) return;

        const package_entry = &self.packages.items[self.selected_index];
        package_entry.install_status = .installing;

        const task_id = try std.fmt.allocPrint(self.allocator, "install_{s}_{d}", .{ package_entry.package.name, std.time.timestamp() });

        const description = try std.fmt.allocPrint(self.allocator, "Installing {s}...", .{package_entry.package.name});

        try self.addBackgroundTask(task_id, description);

        const install_task = try self.runtime.spawn(struct {
            fn install(tui: *PhantomTui, entry: *PackageEntry, bg_task_id: []const u8) !void {
                defer tui.completeBackgroundTask(bg_task_id) catch {};

                // Use appropriate package manager based on source
                const cmd = switch (entry.source) {
                    .aur, .chaotic_aur => try std.fmt.allocPrint(tui.allocator, "yay -S --noconfirm {s}", .{entry.package.name}),
                    .pacman => try std.fmt.allocPrint(tui.allocator, "sudo pacman -S --noconfirm {s}", .{entry.package.name}),
                    .custom => return,
                };
                defer tui.allocator.free(cmd);

                const result = try tui.runtime.spawnSubprocess(.{
                    .argv = &.{ "sh", "-c", cmd },
                    .stdout_behavior = .Pipe,
                    .stderr_behavior = .Pipe,
                });

                if (result.status == 0) {
                    entry.install_status = .installed;
                } else {
                    entry.install_status = .failed;
                }
            }
        }.install, .{ self, package_entry, task_id }, .normal);

        try self.install_tasks.append(self.allocator, install_task);
    }

    fn showDependencies(self: *PhantomTui) !void {
        if (self.selected_index >= self.packages.items.len) return;

        const package_entry = &self.packages.items[self.selected_index];
        self.mode = .dependency_viewer;

        // Load dependencies asynchronously
        const task_id = try std.fmt.allocPrint(self.allocator, "deps_{s}_{d}", .{ package_entry.package.name, std.time.timestamp() });

        try self.addBackgroundTask(task_id, "Loading dependencies...");

        _ = try self.runtime.spawn(struct {
            fn loadDeps(tui: *PhantomTui, entry: *PackageEntry, bg_task_id: []const u8) !void {
                defer tui.completeBackgroundTask(bg_task_id) catch {};

                // Recursively resolve dependencies
                try tui.resolveDependencies(entry);
                try tui.refreshPackageList();
            }
        }.loadDeps, .{ self, package_entry, task_id }, .normal);
    }

    fn resolveDependencies(self: *PhantomTui, entry: *PackageEntry) !void {
        // Implementation would recursively resolve package dependencies
        // using AUR API and pacman database queries
        _ = self;
        _ = entry;
    }

    // Helper functions for background task management
    fn addBackgroundTask(self: *PhantomTui, id: []const u8, description: []const u8) !void {
        try self.background_tasks.append(self.allocator, .{
            .id = try self.allocator.dupe(u8, id),
            .description = try self.allocator.dupe(u8, description),
        });

        try self.task_monitor.addTask(id, description);
    }

    fn updateBackgroundTaskProgress(self: *PhantomTui, id: []const u8, progress: f32) !void {
        for (self.background_tasks.items) |*task| {
            if (std.mem.eql(u8, task.id, id)) {
                task.progress = progress;
                try self.task_monitor.updateTaskProgress(id, progress);
                break;
            }
        }
    }

    fn completeBackgroundTask(self: *PhantomTui, id: []const u8) !void {
        for (self.background_tasks.items, 0..) |task, i| {
            if (std.mem.eql(u8, task.id, id)) {
                self.task_monitor.completeTask(id);
                _ = self.background_tasks.swapRemove(i);
                self.allocator.free(task.id);
                self.allocator.free(task.description);
                break;
            }
        }
    }

    fn updateTaskMonitor(self: *PhantomTui) !void {
        // Update overall progress based on active tasks
        if (self.background_tasks.items.len == 0) {
            try self.progress_bar.setProgress(1.0);
            return;
        }

        var total_progress: f32 = 0.0;
        for (self.background_tasks.items) |task| {
            total_progress += task.progress;
        }

        const average_progress = total_progress / @as(f32, @floatFromInt(self.background_tasks.items.len));
        try self.progress_bar.setProgress(average_progress);
    }

    fn refreshPackageList(self: *PhantomTui) !void {
        try self.package_list.clear();

        for (self.packages.items) |entry| {
            const icon = switch (entry.source) {
                .aur => "📦",
                .pacman => "⚙️",
                .chaotic_aur => "🔮",
                .custom => "🛠️",
            };

            const status_icon = switch (entry.install_status) {
                .not_installed => "○",
                .installed => "●",
                .update_available => "↑",
                .installing => "⟳",
                .failed => "✗",
            };

            const item_text = try std.fmt.allocPrint(self.allocator, "{s} {s} {s} {s} - {s}", .{ icon, status_icon, entry.package.name, entry.package.version, entry.package.description });

            try self.package_list.addItem(item_text);
        }
    }

    fn startSearch(self: *PhantomTui) !void {
        self.mode = .search_results;
        self.search_query.clearRetainingCapacity();
    }

    fn handleEnterKey(self: *PhantomTui) !void {
        switch (self.mode) {
            .search_results => try self.searchPackagesAsync(self.search_query.items),
            .package_browser => try self.showDependencies(),
            .dependency_viewer => try self.installSelectedPackage(),
            else => {},
        }
    }

    fn switchMode(self: *PhantomTui) !void {
        self.mode = switch (self.mode) {
            .package_browser => .search_results,
            .search_results => .dependency_viewer,
            .dependency_viewer => .repository_explorer,
            .repository_explorer => .package_browser,
            .install_progress => .package_browser,
        };
    }

    fn uninstallSelectedPackage(self: *PhantomTui) !void {
        // Similar to installSelectedPackage but for removal
        _ = self;
    }
};
