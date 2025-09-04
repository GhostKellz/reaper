const std = @import("std");
const zsync = @import("zsync");

pub const BuildError = error{
    InvalidPkgbuild,
    BuildFailed,
    SourceDownloadFailed,
    DependencyMissing,
    InsufficientResources,
    SignatureVerificationFailed,
    SandboxError,
} || std.mem.Allocator.Error;

pub const BuildPriority = enum(u8) {
    low = 0,
    normal = 1,
    high = 2,
    critical = 3,
};

pub const BuildJobStatus = enum {
    queued,
    downloading_sources,
    resolving_dependencies,
    building,
    packaging,
    verifying,
    completed,
    failed,
    cancelled,
};

pub const BuildJob = struct {
    id: []const u8,
    package_name: []const u8,
    pkgbuild_path: []const u8,
    work_dir: []const u8,
    priority: BuildPriority,
    status: std.atomic.Value(BuildJobStatus),
    progress: std.atomic.Value(f32),
    start_time: i64,
    dependencies: [][]const u8,
    sources: []SourceFile,
    build_options: BuildOptions,
    
    // Resource tracking
    cpu_cores_used: u8,
    memory_mb_used: u32,
    disk_mb_used: u32,
    
    // Callbacks
    progress_callback: ?*const fn(job: *BuildJob, status: BuildJobStatus, progress: f32) void,
    completion_callback: ?*const fn(job: *BuildJob, result: BuildResult) void,

    const SourceFile = struct {
        url: []const u8,
        filename: []const u8,
        checksum: ?[]const u8,
        signature: ?[]const u8,
        downloaded: std.atomic.Value(bool),
        verified: std.atomic.Value(bool),
    };

    const BuildOptions = struct {
        use_ccache: bool = true,
        parallel_make: bool = true,
        debug_symbols: bool = false,
        optimize_level: u8 = 2,
        enable_lto: bool = false,
        strip_binaries: bool = true,
        run_tests: bool = true,
        create_debug_package: bool = false,
        sign_package: bool = false,
        compression_level: u8 = 6,
    };

    pub fn init(allocator: std.mem.Allocator, package_name: []const u8, pkgbuild_path: []const u8) !*BuildJob {
        const job = try allocator.create(BuildJob);
        const timestamp = std.time.timestamp();
        
        job.* = BuildJob{
            .id = try std.fmt.allocPrint(allocator, "build_{s}_{d}", .{ package_name, timestamp }),
            .package_name = try allocator.dupe(u8, package_name),
            .pkgbuild_path = try allocator.dupe(u8, pkgbuild_path),
            .work_dir = try std.fmt.allocPrint(allocator, "/tmp/reaper_build_{s}_{d}", .{ package_name, timestamp }),
            .priority = .normal,
            .status = std.atomic.Value(BuildJobStatus).init(.queued),
            .progress = std.atomic.Value(f32).init(0.0),
            .start_time = std.time.milliTimestamp(),
            .dependencies = &.{},
            .sources = &.{},
            .build_options = BuildOptions{},
            .cpu_cores_used = 1,
            .memory_mb_used = 512,
            .disk_mb_used = 1024,
            .progress_callback = null,
            .completion_callback = null,
        };
        
        return job;
    }

    pub fn deinit(self: *BuildJob, allocator: std.mem.Allocator) void {
        allocator.free(self.id);
        allocator.free(self.package_name);
        allocator.free(self.pkgbuild_path);
        allocator.free(self.work_dir);
        
        for (self.dependencies) |dep| {
            allocator.free(dep);
        }
        allocator.free(self.dependencies);
        
        for (self.sources) |source| {
            allocator.free(source.url);
            allocator.free(source.filename);
            if (source.checksum) |checksum| allocator.free(checksum);
            if (source.signature) |signature| allocator.free(signature);
        }
        allocator.free(self.sources);
        
        allocator.destroy(self);
    }

    pub fn updateProgress(self: *BuildJob, status: BuildJobStatus, progress: f32) void {
        self.status.store(status, .release);
        self.progress.store(progress, .release);
        
        if (self.progress_callback) |callback| {
            callback(self, status, progress);
        }
    }

    pub fn getEstimatedTimeRemaining(self: *const BuildJob) u64 {
        const elapsed = @as(u64, @intCast(std.time.milliTimestamp() - self.start_time));
        const current_progress = self.progress.load(.acquire);
        
        if (current_progress <= 0.0) return 0;
        
        const total_estimated = @as(u64, @intFromFloat(@as(f64, @floatFromInt(elapsed)) / current_progress));
        return if (total_estimated > elapsed) total_estimated - elapsed else 0;
    }
};

pub const BuildResult = struct {
    job_id: []const u8,
    success: bool,
    package_files: [][]const u8,
    log_output: []const u8,
    error_message: ?[]const u8,
    build_time_ms: u64,
    artifacts: BuildArtifacts,
    allocator: std.mem.Allocator,

    const BuildArtifacts = struct {
        debug_symbols: ?[]const u8,
        source_package: ?[]const u8,
        signatures: [][]const u8,
        checksums: [][]const u8,
        build_log: []const u8,
        install_log: []const u8,
    };

    pub fn deinit(self: *BuildResult) void {
        self.allocator.free(self.job_id);
        
        for (self.package_files) |file| {
            self.allocator.free(file);
        }
        self.allocator.free(self.package_files);
        
        self.allocator.free(self.log_output);
        if (self.error_message) |msg| self.allocator.free(msg);
        
        if (self.artifacts.debug_symbols) |debug| self.allocator.free(debug);
        if (self.artifacts.source_package) |src| self.allocator.free(src);
        
        for (self.artifacts.signatures) |sig| {
            self.allocator.free(sig);
        }
        self.allocator.free(self.artifacts.signatures);
        
        for (self.artifacts.checksums) |checksum| {
            self.allocator.free(checksum);
        }
        self.allocator.free(self.artifacts.checksums);
        
        self.allocator.free(self.artifacts.build_log);
        self.allocator.free(self.artifacts.install_log);
    }
};

pub const ResourceManager = struct {
    max_cpu_cores: u8,
    max_memory_mb: u32,
    max_disk_mb: u32,
    current_cpu_cores: std.atomic.Value(u8),
    current_memory_mb: std.atomic.Value(u32),
    current_disk_mb: std.atomic.Value(u32),
    mutex: std.Thread.Mutex,

    pub fn init(max_cores: u8, max_memory_mb: u32, max_disk_mb: u32) ResourceManager {
        return ResourceManager{
            .max_cpu_cores = max_cores,
            .max_memory_mb = max_memory_mb,
            .max_disk_mb = max_disk_mb,
            .current_cpu_cores = std.atomic.Value(u8).init(0),
            .current_memory_mb = std.atomic.Value(u32).init(0),
            .current_disk_mb = std.atomic.Value(u32).init(0),
            .mutex = std.Thread.Mutex{},
        };
    }

    pub fn canAllocate(self: *ResourceManager, job: *const BuildJob) bool {
        const current_cores = self.current_cpu_cores.load(.acquire);
        const current_memory = self.current_memory_mb.load(.acquire);
        const current_disk = self.current_disk_mb.load(.acquire);

        return (current_cores + job.cpu_cores_used <= self.max_cpu_cores) and
               (current_memory + job.memory_mb_used <= self.max_memory_mb) and
               (current_disk + job.disk_mb_used <= self.max_disk_mb);
    }

    pub fn allocateResources(self: *ResourceManager, job: *const BuildJob) bool {
        self.mutex.lock();
        defer self.mutex.unlock();

        if (!self.canAllocate(job)) {
            return false;
        }

        _ = self.current_cpu_cores.fetchAdd(job.cpu_cores_used, .acq_rel);
        _ = self.current_memory_mb.fetchAdd(job.memory_mb_used, .acq_rel);
        _ = self.current_disk_mb.fetchAdd(job.disk_mb_used, .acq_rel);

        return true;
    }

    pub fn releaseResources(self: *ResourceManager, job: *const BuildJob) void {
        _ = self.current_cpu_cores.fetchSub(job.cpu_cores_used, .acq_rel);
        _ = self.current_memory_mb.fetchSub(job.memory_mb_used, .acq_rel);
        _ = self.current_disk_mb.fetchSub(job.disk_mb_used, .acq_rel);
    }

    pub fn getUtilization(self: *const ResourceManager) struct { cpu: f32, memory: f32, disk: f32 } {
        const cpu_usage = @as(f32, @floatFromInt(self.current_cpu_cores.load(.acquire))) / @as(f32, @floatFromInt(self.max_cpu_cores));
        const memory_usage = @as(f32, @floatFromInt(self.current_memory_mb.load(.acquire))) / @as(f32, @floatFromInt(self.max_memory_mb));
        const disk_usage = @as(f32, @floatFromInt(self.current_disk_mb.load(.acquire))) / @as(f32, @floatFromInt(self.max_disk_mb));

        return .{ .cpu = cpu_usage, .memory = memory_usage, .disk = disk_usage };
    }
};

pub const BuildQueue = struct {
    jobs: std.PriorityQueue(*BuildJob, void, compareBuildJobs),
    mutex: std.Thread.Mutex,
    condition: std.Thread.Condition,
    allocator: std.mem.Allocator,

    fn compareBuildJobs(context: void, a: *BuildJob, b: *BuildJob) std.math.Order {
        _ = context;
        
        // Higher priority jobs come first
        if (@intFromEnum(a.priority) > @intFromEnum(b.priority)) return .lt;
        if (@intFromEnum(a.priority) < @intFromEnum(b.priority)) return .gt;
        
        // For same priority, FIFO based on start time
        if (a.start_time < b.start_time) return .lt;
        if (a.start_time > b.start_time) return .gt;
        
        return .eq;
    }

    pub fn init(allocator: std.mem.Allocator) BuildQueue {
        return BuildQueue{
            .jobs = std.PriorityQueue(*BuildJob, void, compareBuildJobs).init(allocator, {}),
            .mutex = std.Thread.Mutex{},
            .condition = std.Thread.Condition{},
            .allocator = allocator,
        };
    }

    pub fn deinit(self: *BuildQueue) void {
        self.jobs.deinit();
    }

    pub fn enqueue(self: *BuildQueue, job: *BuildJob) void {
        self.mutex.lock();
        defer self.mutex.unlock();

        self.jobs.add(job) catch return; // TODO: handle error properly
        self.condition.broadcast();
    }

    pub fn dequeue(self: *BuildQueue, timeout_ms: u64) ?*BuildJob {
        self.mutex.lock();
        defer self.mutex.unlock();

        const deadline = std.time.milliTimestamp() + @as(i64, @intCast(timeout_ms));

        while (self.jobs.count() == 0) {
            const now = std.time.milliTimestamp();
            if (now >= deadline) return null;

            const remaining_ms = @as(u64, @intCast(deadline - now));
            self.condition.timedWait(&self.mutex, remaining_ms * std.time.ns_per_ms) catch return null;
        }

        return self.jobs.remove();
    }

    pub fn size(self: *const BuildQueue) usize {
        return self.jobs.count();
    }
};

pub const ParallelBuilder = struct {
    allocator: std.mem.Allocator,
    io: zsync.Io,
    build_queue: BuildQueue,
    resource_manager: ResourceManager,
    active_jobs: std.ArrayList(*BuildJob),
    worker_threads: []std.Thread,
    shutdown: std.atomic.Value(bool),
    stats: BuildStats,

    const BuildStats = struct {
        total_jobs: std.atomic.Value(u64),
        completed_jobs: std.atomic.Value(u64),
        failed_jobs: std.atomic.Value(u64),
        total_build_time_ms: std.atomic.Value(u64),
        bytes_downloaded: std.atomic.Value(u64),
        packages_built: std.atomic.Value(u64),
    };

    pub fn init(allocator: std.mem.Allocator, io: zsync.Io, max_workers: u8) !*ParallelBuilder {
        const builder = try allocator.create(ParallelBuilder);
        
        // Detect system resources
        const cpu_count = std.Thread.getCpuCount() catch 4;
        const max_cores = @min(cpu_count, max_workers);
        
        builder.* = ParallelBuilder{
            .allocator = allocator,
            .io = io,
            .build_queue = BuildQueue.init(allocator),
            .resource_manager = ResourceManager.init(
                @intCast(max_cores),
                4096, // 4GB memory limit
                10240, // 10GB disk limit
            ),
            .active_jobs = std.ArrayList(*BuildJob){},
            .worker_threads = try allocator.alloc(std.Thread, max_workers),
            .shutdown = std.atomic.Value(bool).init(false),
            .stats = BuildStats{
                .total_jobs = std.atomic.Value(u64).init(0),
                .completed_jobs = std.atomic.Value(u64).init(0),
                .failed_jobs = std.atomic.Value(u64).init(0),
                .total_build_time_ms = std.atomic.Value(u64).init(0),
                .bytes_downloaded = std.atomic.Value(u64).init(0),
                .packages_built = std.atomic.Value(u64).init(0),
            },
        };

        // Start worker threads
        for (builder.worker_threads, 0..) |*thread, i| {
            thread.* = try std.Thread.spawn(.{}, workerMain, .{ builder, i });
        }

        return builder;
    }

    pub fn deinit(self: *ParallelBuilder) void {
        // Signal shutdown
        self.shutdown.store(true, .release);
        
        // Wake up all workers
        self.build_queue.condition.broadcast();
        
        // Wait for workers to finish
        for (self.worker_threads) |thread| {
            thread.join();
        }
        
        // Cleanup
        self.allocator.free(self.worker_threads);
        self.build_queue.deinit();
        self.active_jobs.deinit(allocator);
        self.allocator.destroy(self);
    }

    pub fn submitBuild(self: *ParallelBuilder, package_name: []const u8, pkgbuild_path: []const u8) !*BuildJob {
        const job = try BuildJob.init(self.allocator, package_name, pkgbuild_path);
        
        // Parse PKGBUILD to extract build info
        try self.analyzePkgbuild(job);
        
        // Enqueue the job
        self.build_queue.enqueue(job);
        _ = self.stats.total_jobs.fetchAdd(1, .acq_rel);
        
        return job;
    }

    fn workerMain(self: *ParallelBuilder, worker_id: usize) void {
        std.log.info("Build worker {} started", .{worker_id});
        
        while (!self.shutdown.load(.acquire)) {
            // Wait for a job
            if (self.build_queue.dequeue(1000)) |job| {
                // Check if we can allocate resources
                if (self.resource_manager.allocateResources(job)) {
                    // Add to active jobs
                    self.active_jobs.append(allocator, job) catch continue;
                    
                    // Execute the build
                    const result = self.executeJob(job) catch |err| BuildResult{
                        .job_id = try self.allocator.dupe(u8, job.id),
                        .success = false,
                        .package_files = &.{},
                        .log_output = try self.allocator.dupe(u8, "Build execution failed"),
                        .error_message = try self.allocator.dupe(u8, @errorName(err)),
                        .build_time_ms = 0,
                        .artifacts = BuildResult.BuildArtifacts{
                            .debug_symbols = null,
                            .source_package = null,
                            .signatures = &.{},
                            .checksums = &.{},
                            .build_log = try self.allocator.dupe(u8, ""),
                            .install_log = try self.allocator.dupe(u8, ""),
                        },
                        .allocator = self.allocator,
                    };
                    
                    // Update statistics
                    if (result.success) {
                        _ = self.stats.completed_jobs.fetchAdd(1, .acq_rel);
                        _ = self.stats.packages_built.fetchAdd(1, .acq_rel);
                    } else {
                        _ = self.stats.failed_jobs.fetchAdd(1, .acq_rel);
                    }
                    _ = self.stats.total_build_time_ms.fetchAdd(result.build_time_ms, .acq_rel);
                    
                    // Call completion callback
                    if (job.completion_callback) |callback| {
                        callback(job, result);
                    }
                    
                    // Cleanup
                    self.resource_manager.releaseResources(job);
                    
                    // Remove from active jobs
                    for (self.active_jobs.items, 0..) |active_job, i| {
                        if (active_job == job) {
                            _ = self.active_jobs.swapRemove(i);
                            break;
                        }
                    }
                    
                    result.deinit();
                } else {
                    // Resources not available, re-queue the job
                    self.build_queue.enqueue(job);
                    std.Thread.sleep(100 * std.time.ns_per_ms); // Wait 100ms before retrying
                }
            }
        }
        
        std.log.info("Build worker {} stopped", .{worker_id});
    }

    fn analyzePkgbuild(self: *ParallelBuilder, job: *BuildJob) !void {
        // Read and parse PKGBUILD
        const file = try std.fs.openFileAbsolute(job.pkgbuild_path, .{});
        defer file.close();
        
        const content = try file.readToEndAlloc(self.allocator, 1024 * 1024); // 1MB limit
        defer self.allocator.free(content);
        
        // Extract build dependencies, sources, etc.
        // This is a simplified parser - real implementation would be more robust
        var sources = std.ArrayList(BuildJob.SourceFile){};
        defer sources.deinit(self.allocator);
        
        var dependencies = std.ArrayList([]const u8){};
        defer dependencies.deinit(self.allocator);
        
        // Parse sources
        if (std.mem.indexOf(u8, content, "source=(")) |source_start| {
            // Extract source array - simplified parsing
            const line_end = std.mem.indexOf(u8, content[source_start..], "\n") orelse content.len - source_start;
            const source_line = content[source_start..source_start + line_end];
            
            // Very basic URL extraction
            var url_start: usize = 0;
            while (std.mem.indexOf(u8, source_line[url_start..], "http")) |http_pos| {
                const absolute_pos = url_start + http_pos;
                const url_end = std.mem.indexOfAny(u8, source_line[absolute_pos..], " \t\n\")") orelse source_line.len - absolute_pos;
                const url = source_line[absolute_pos..absolute_pos + url_end];
                
                if (url.len > 0) {
                    const filename = std.fs.path.basename(url);
                    try sources.append(self.allocator, .{
                        .url = try self.allocator.dupe(u8, url),
                        .filename = try self.allocator.dupe(u8, filename),
                        .checksum = null,
                        .signature = null,
                        .downloaded = std.atomic.Value(bool).init(false),
                        .verified = std.atomic.Value(bool).init(false),
                    });
                }
                
                url_start = absolute_pos + url_end;
            }
        }
        
        // Parse makedepends
        if (std.mem.indexOf(u8, content, "makedepends=(")) |makedep_start| {
            // Extract makedepends array - simplified parsing
            const line_end = std.mem.indexOf(u8, content[makedep_start..], "\n") orelse content.len - makedep_start;
            const makedep_line = content[makedep_start..makedep_start + line_end];
            
            // Basic dependency extraction
            var dep_start: usize = 0;
            while (std.mem.indexOf(u8, makedep_line[dep_start..], "'")) |quote_pos| {
                const absolute_pos = dep_start + quote_pos + 1;
                const dep_end = std.mem.indexOf(u8, makedep_line[absolute_pos..], "'") orelse continue;
                const dep = makedep_line[absolute_pos..absolute_pos + dep_end];
                
                if (dep.len > 0) {
                    try dependencies.append(self.allocator, try self.allocator.dupe(u8, dep));
                }
                
                dep_start = absolute_pos + dep_end + 1;
            }
        }
        
        // Estimate resource requirements based on package complexity
        job.cpu_cores_used = @min(@max(1, @as(u8, @intCast(sources.items.len / 2))), 4);
        job.memory_mb_used = 512 + @as(u32, @intCast(sources.items.len * 128));
        job.disk_mb_used = 1024 + @as(u32, @intCast(sources.items.len * 256));
        
        // Store parsed data
        job.sources = try sources.toOwnedSlice();
        job.dependencies = try dependencies.toOwnedSlice();
    }

    fn executeJob(self: *ParallelBuilder, job: *BuildJob) !BuildResult {
        const start_time = std.time.milliTimestamp();
        
        // Create work directory
        try std.fs.makeDirAbsolute(job.work_dir);
        defer std.fs.deleteTreeAbsolute(job.work_dir) catch {};
        
        var build_log = std.ArrayList(u8){};
        defer build_log.deinit(self.allocator);
        
        // Step 1: Download sources
        job.updateProgress(.downloading_sources, 0.1);
        try self.downloadSources(job, &build_log);
        
        // Step 2: Verify checksums and signatures
        job.updateProgress(.verifying, 0.2);
        try self.verifySources(job, &build_log);
        
        // Step 3: Resolve dependencies
        job.updateProgress(.resolving_dependencies, 0.3);
        try self.resolveDependencies(job, &build_log);
        
        // Step 4: Execute build
        job.updateProgress(.building, 0.4);
        const package_files = try self.executeBuild(job, &build_log);
        
        // Step 5: Package artifacts
        job.updateProgress(.packaging, 0.8);
        try self.packageArtifacts(job, &build_log);
        
        job.updateProgress(.completed, 1.0);
        
        const build_time = @as(u64, @intCast(std.time.milliTimestamp() - start_time));
        
        return BuildResult{
            .job_id = try self.allocator.dupe(u8, job.id),
            .success = true,
            .package_files = package_files,
            .log_output = try build_log.toOwnedSlice(),
            .error_message = null,
            .build_time_ms = build_time,
            .artifacts = BuildResult.BuildArtifacts{
                .debug_symbols = null,
                .source_package = null,
                .signatures = &.{},
                .checksums = &.{},
                .build_log = try self.allocator.dupe(u8, ""),
                .install_log = try self.allocator.dupe(u8, ""),
            },
            .allocator = self.allocator,
        };
    }

    fn downloadSources(self: *ParallelBuilder, job: *BuildJob, build_log: *std.ArrayList(u8)) !void {
        for (job.sources) |*source| {
            try build_log.writer().print("Downloading: {s}\n", .{source.url});
            
            // Create download task (simplified - would use async HTTP in real implementation)
            const download_path = try std.fs.path.join(self.allocator, &.{ job.work_dir, source.filename });
            defer self.allocator.free(download_path);
            
            // Simulate download with curl for now
            const curl_cmd = try std.fmt.allocPrint(self.allocator, "curl -L -o '{s}' '{s}'", .{ download_path, source.url });
            defer self.allocator.free(curl_cmd);
            
            const result = try std.process.Child.run(.{
                .allocator = self.allocator,
                .argv = &.{ "sh", "-c", curl_cmd },
            });
            defer self.allocator.free(result.stdout);
            defer self.allocator.free(result.stderr);
            
            if (result.term != .Exited or result.term.Exited != 0) {
                return BuildError.SourceDownloadFailed;
            }
            
            source.downloaded.store(true, .release);
            _ = self.stats.bytes_downloaded.fetchAdd(1024, .acq_rel); // Placeholder
        }
    }

    fn verifySources(self: *ParallelBuilder, job: *BuildJob, build_log: *std.ArrayList(u8)) !void {
        _ = self;
        for (job.sources) |*source| {
            try build_log.writer().print("Verifying: {s}\n", .{source.filename});
            
            // TODO: Implement checksum and signature verification
            source.verified.store(true, .release);
        }
    }

    fn resolveDependencies(self: *ParallelBuilder, job: *BuildJob, build_log: *std.ArrayList(u8)) !void {
        _ = self;
        for (job.dependencies) |dep| {
            try build_log.writer().print("Checking dependency: {s}\n", .{dep});
            
            // TODO: Check if dependency is installed
            // TODO: Install missing dependencies
        }
    }

    fn executeBuild(self: *ParallelBuilder, job: *BuildJob, build_log: *std.ArrayList(u8)) ![][]const u8 {
        _ = self;
        
        try build_log.writer().print("Starting build for: {s}\n", .{job.package_name});
        
        // Copy PKGBUILD to work directory
        const dest_pkgbuild = try std.fs.path.join(self.allocator, &.{ job.work_dir, "PKGBUILD" });
        defer self.allocator.free(dest_pkgbuild);
        
        try std.fs.copyFileAbsolute(job.pkgbuild_path, dest_pkgbuild, .{});
        
        // Execute makepkg
        const makepkg_cmd = if (job.build_options.parallel_make)
            "makepkg -s --noconfirm --noprogressbar"
        else
            "makepkg -s --noconfirm --noprogressbar -j1";
        
        const result = try std.process.Child.run(.{
            .allocator = self.allocator,
            .argv = &.{ "sh", "-c", makepkg_cmd },
            .cwd = job.work_dir,
        });
        defer self.allocator.free(result.stdout);
        defer self.allocator.free(result.stderr);
        
        try build_log.appendSlice(self.allocator, result.stdout);
        try build_log.appendSlice(self.allocator, result.stderr);
        
        if (result.term != .Exited or result.term.Exited != 0) {
            return BuildError.BuildFailed;
        }
        
        // Find generated package files
        var package_files = std.ArrayList([]const u8){};
        defer package_files.deinit(self.allocator);
        
        var dir = try std.fs.openDirAbsolute(job.work_dir, .{ .iterate = true });
        defer dir.close();
        
        var iterator = dir.iterate();
        while (try iterator.next()) |entry| {
            if (entry.kind == .file and std.mem.endsWith(u8, entry.name, ".pkg.tar.zst")) {
                const full_path = try std.fs.path.join(self.allocator, &.{ job.work_dir, entry.name });
                try package_files.append(self.allocator, full_path);
            }
        }
        
        return package_files.toOwnedSlice();
    }

    fn packageArtifacts(self: *ParallelBuilder, job: *BuildJob, build_log: *std.ArrayList(u8)) !void {
        _ = self;
        _ = job;
        try build_log.writer().print("Packaging build artifacts\n", .{});
        
        // TODO: Create debug packages, source packages, signatures, etc.
    }

    // Statistics and monitoring
    pub fn getStats(self: *const ParallelBuilder) struct {
        total_jobs: u64,
        completed_jobs: u64,
        failed_jobs: u64,
        active_jobs: usize,
        queue_size: usize,
        resource_utilization: struct { cpu: f32, memory: f32, disk: f32 },
        average_build_time_ms: u64,
    } {
        const total = self.stats.total_jobs.load(.acquire);
        const completed = self.stats.completed_jobs.load(.acquire);
        const total_time = self.stats.total_build_time_ms.load(.acquire);
        
        return .{
            .total_jobs = total,
            .completed_jobs = completed,
            .failed_jobs = self.stats.failed_jobs.load(.acquire),
            .active_jobs = self.active_jobs.items.len,
            .queue_size = self.build_queue.size(),
            .resource_utilization = self.resource_manager.getUtilization(),
            .average_build_time_ms = if (completed > 0) total_time / completed else 0,
        };
    }
};