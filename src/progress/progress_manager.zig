const std = @import("std");
const zsync = @import("zsync");

pub const ProgressError = error{
    InvalidProgressValue,
    TaskNotFound,
    AlreadyCompleted,
    Cancelled,
} || std.mem.Allocator.Error;

pub const ProgressPhase = enum {
    initializing,
    downloading,
    extracting,
    building,
    installing,
    cleaning,
    completed,
    failed,
    cancelled,
    
    pub fn toString(self: ProgressPhase) []const u8 {
        return switch (self) {
            .initializing => "Initializing",
            .downloading => "Downloading",
            .extracting => "Extracting",
            .building => "Building", 
            .installing => "Installing",
            .cleaning => "Cleaning up",
            .completed => "Completed",
            .failed => "Failed",
            .cancelled => "Cancelled",
        };
    }
};

pub const ProgressUpdate = struct {
    task_id: []const u8,
    phase: ProgressPhase,
    progress: f32, // 0.0 to 1.0
    message: []const u8,
    details: ?ProgressDetails = null,
    timestamp: i64,
    
    pub const ProgressDetails = union(enum) {
        download: struct {
            bytes_downloaded: u64,
            total_bytes: u64,
            speed_bps: u64,
            eta_seconds: u64,
        },
        build: struct {
            current_step: []const u8,
            total_steps: u32,
            current_step_progress: f32,
        },
        install: struct {
            files_processed: u32,
            total_files: u32,
            current_file: []const u8,
        },
        error_info: struct {
            error_code: []const u8,
            error_message: []const u8,
            stack_trace: ?[]const u8,
        },
    };
    
    pub fn init(allocator: std.mem.Allocator, task_id: []const u8, phase: ProgressPhase, progress: f32, message: []const u8) !ProgressUpdate {
        return ProgressUpdate{
            .task_id = try allocator.dupe(u8, task_id),
            .phase = phase,
            .progress = std.math.clamp(progress, 0.0, 1.0),
            .message = try allocator.dupe(u8, message),
            .details = null,
            .timestamp = std.time.milliTimestamp(),
        };
    }
    
    pub fn deinit(self: *ProgressUpdate, allocator: std.mem.Allocator) void {
        allocator.free(self.task_id);
        allocator.free(self.message);
        
        if (self.details) |*details| {
            switch (details.*) {
                .build => |*build_details| {
                    allocator.free(build_details.current_step);
                },
                .install => |*install_details| {
                    allocator.free(install_details.current_file);
                },
                .error_info => |*error_details| {
                    allocator.free(error_details.error_code);
                    allocator.free(error_details.error_message);
                    if (error_details.stack_trace) |trace| {
                        allocator.free(trace);
                    }
                },
                .download => {},
            }
        }
    }
    
    pub fn clone(self: *const ProgressUpdate, allocator: std.mem.Allocator) !ProgressUpdate {
        var cloned = ProgressUpdate{
            .task_id = try allocator.dupe(u8, self.task_id),
            .phase = self.phase,
            .progress = self.progress,
            .message = try allocator.dupe(u8, self.message),
            .details = null,
            .timestamp = self.timestamp,
        };
        
        if (self.details) |details| {
            cloned.details = switch (details) {
                .download => |d| ProgressUpdate.ProgressDetails{ .download = d },
                .build => |b| ProgressUpdate.ProgressDetails{ 
                    .build = .{
                        .current_step = try allocator.dupe(u8, b.current_step),
                        .total_steps = b.total_steps,
                        .current_step_progress = b.current_step_progress,
                    }
                },
                .install => |i| ProgressUpdate.ProgressDetails{
                    .install = .{
                        .files_processed = i.files_processed,
                        .total_files = i.total_files,
                        .current_file = try allocator.dupe(u8, i.current_file),
                    }
                },
                .error_info => |e| ProgressUpdate.ProgressDetails{
                    .error_info = .{
                        .error_code = try allocator.dupe(u8, e.error_code),
                        .error_message = try allocator.dupe(u8, e.error_message),
                        .stack_trace = if (e.stack_trace) |trace| try allocator.dupe(u8, trace) else null,
                    }
                },
            };
        }
        
        return cloned;
    }
};

pub const ProgressSubscriber = struct {
    id: []const u8,
    callback: *const fn(update: ProgressUpdate, user_data: ?*anyopaque) void,
    filter: ?ProgressFilter,
    user_data: ?*anyopaque,
    
    pub const ProgressFilter = struct {
        task_ids: ?[][]const u8 = null,
        phases: ?[]ProgressPhase = null,
        min_progress_delta: f32 = 0.01, // Only report if progress changed by at least 1%
    };
    
    pub fn init(allocator: std.mem.Allocator, id: []const u8, callback: *const fn(update: ProgressUpdate, user_data: ?*anyopaque) void) !ProgressSubscriber {
        return ProgressSubscriber{
            .id = try allocator.dupe(u8, id),
            .callback = callback,
            .filter = null,
            .user_data = null,
        };
    }
    
    pub fn deinit(self: *ProgressSubscriber, allocator: std.mem.Allocator) void {
        allocator.free(self.id);
        
        if (self.filter) |*filter| {
            if (filter.task_ids) |task_ids| {
                for (task_ids) |task_id| {
                    allocator.free(task_id);
                }
                allocator.free(task_ids);
            }
            
            if (filter.phases) |phases| {
                allocator.free(phases);
            }
        }
    }
    
    pub fn shouldReceiveUpdate(self: *const ProgressSubscriber, update: *const ProgressUpdate, last_progress: f32) bool {
        if (self.filter) |filter| {
            // Check task ID filter
            if (filter.task_ids) |task_ids| {
                var found = false;
                for (task_ids) |task_id| {
                    if (std.mem.eql(u8, task_id, update.task_id)) {
                        found = true;
                        break;
                    }
                }
                if (!found) return false;
            }
            
            // Check phase filter
            if (filter.phases) |phases| {
                var found = false;
                for (phases) |phase| {
                    if (phase == update.phase) {
                        found = true;
                        break;
                    }
                }
                if (!found) return false;
            }
            
            // Check progress delta
            const progress_delta = @abs(update.progress - last_progress);
            if (progress_delta < filter.min_progress_delta and 
               update.phase != .completed and 
               update.phase != .failed and 
               update.phase != .cancelled) {
                return false;
            }
        }
        
        return true;
    }
};

pub const TaskState = struct {
    id: []const u8,
    name: []const u8,
    phase: std.atomic.Value(ProgressPhase),
    progress: std.atomic.Value(f32),
    last_update: std.atomic.Value(i64),
    start_time: i64,
    cancellation_token: *zsync.CancelToken,
    parent_task_id: ?[]const u8,
    subtasks: std.ArrayList([]const u8),
    
    pub fn init(allocator: std.mem.Allocator, id: []const u8, name: []const u8, cancel_token: *zsync.CancelToken) !*TaskState {
        const state = try allocator.create(TaskState);
        state.* = TaskState{
            .id = try allocator.dupe(u8, id),
            .name = try allocator.dupe(u8, name),
            .phase = std.atomic.Value(ProgressPhase).init(.initializing),
            .progress = std.atomic.Value(f32).init(0.0),
            .last_update = std.atomic.Value(i64).init(std.time.milliTimestamp()),
            .start_time = std.time.milliTimestamp(),
            .cancellation_token = cancel_token,
            .parent_task_id = null,
            .subtasks = std.ArrayList([]const u8).init(allocator),
        };
        return state;
    }
    
    pub fn deinit(self: *TaskState, allocator: std.mem.Allocator) void {
        allocator.free(self.id);
        allocator.free(self.name);
        
        if (self.parent_task_id) |parent_id| {
            allocator.free(parent_id);
        }
        
        for (self.subtasks.items) |subtask_id| {
            allocator.free(subtask_id);
        }
        self.subtasks.deinit();
        
        allocator.destroy(self);
    }
    
    pub fn isCancelled(self: *const TaskState) bool {
        return self.cancellation_token.isCancelled();
    }
    
    pub fn cancel(self: *TaskState) void {
        self.cancellation_token.cancel();
        self.phase.store(.cancelled, .release);
    }
    
    pub fn getElapsedTime(self: *const TaskState) u64 {
        return @as(u64, @intCast(std.time.milliTimestamp() - self.start_time));
    }
    
    pub fn getEstimatedTimeRemaining(self: *const TaskState) ?u64 {
        const current_progress = self.progress.load(.acquire);
        if (current_progress <= 0.0) return null;
        
        const elapsed = self.getElapsedTime();
        const total_estimated = @as(u64, @intFromFloat(@as(f64, @floatFromInt(elapsed)) / current_progress));
        
        return if (total_estimated > elapsed) total_estimated - elapsed else 0;
    }
    
    pub fn addSubtask(self: *TaskState, allocator: std.mem.Allocator, subtask_id: []const u8) !void {
        try self.subtasks.append(try allocator.dupe(u8, subtask_id));
    }
};

pub const ProgressManager = struct {
    allocator: std.mem.Allocator,
    io: zsync.Io,
    tasks: std.StringHashMap(*TaskState),
    subscribers: std.ArrayList(ProgressSubscriber),
    last_progress: std.StringHashMap(f32),
    update_queue: zsync.realtime_streams.RealtimeStream,
    worker_thread: std.Thread,
    shutdown: std.atomic.Value(bool),
    stats: ProgressStats,
    mutex: std.Thread.RwLock,
    
    const ProgressStats = struct {
        total_tasks: std.atomic.Value(u64),
        completed_tasks: std.atomic.Value(u64),
        cancelled_tasks: std.atomic.Value(u64),
        failed_tasks: std.atomic.Value(u64),
        updates_sent: std.atomic.Value(u64),
        subscribers_count: std.atomic.Value(u32),
    };
    
    pub fn init(allocator: std.mem.Allocator, io: zsync.Io) !*ProgressManager {
        const manager = try allocator.create(ProgressManager);
        
        const stream_config = zsync.realtime_streams.StreamConfig{
            .buffer_size = 1024 * 16, // 16KB buffer
            .enable_backpressure = true,
            .max_pending_items = 1000,
        };
        
        manager.* = ProgressManager{
            .allocator = allocator,
            .io = io,
            .tasks = std.StringHashMap(*TaskState).init(allocator),
            .subscribers = std.ArrayList(ProgressSubscriber).init(allocator),
            .last_progress = std.StringHashMap(f32).init(allocator),
            .update_queue = try zsync.realtime_streams.RealtimeStream.builder()
                .config(stream_config)
                .build(allocator),
            .worker_thread = undefined,
            .shutdown = std.atomic.Value(bool).init(false),
            .stats = ProgressStats{
                .total_tasks = std.atomic.Value(u64).init(0),
                .completed_tasks = std.atomic.Value(u64).init(0),
                .cancelled_tasks = std.atomic.Value(u64).init(0),
                .failed_tasks = std.atomic.Value(u64).init(0),
                .updates_sent = std.atomic.Value(u64).init(0),
                .subscribers_count = std.atomic.Value(u32).init(0),
            },
            .mutex = std.Thread.RwLock{},
        };
        
        // Start worker thread for processing updates
        manager.worker_thread = try std.Thread.spawn(.{}, progressWorkerMain, .{manager});
        
        return manager;
    }
    
    pub fn deinit(self: *ProgressManager) void {
        // Signal shutdown
        self.shutdown.store(true, .release);
        
        // Send shutdown signal to stream
        self.update_queue.close() catch {};
        
        // Wait for worker thread
        self.worker_thread.join();
        
        // Cleanup tasks
        self.mutex.lock();
        var task_iter = self.tasks.iterator();
        while (task_iter.next()) |entry| {
            entry.value_ptr.*.deinit(self.allocator);
            self.allocator.free(entry.key_ptr.*);
        }
        self.tasks.deinit();
        
        // Cleanup subscribers
        for (self.subscribers.items) |*subscriber| {
            subscriber.deinit(self.allocator);
        }
        self.subscribers.deinit();
        
        // Cleanup progress tracking
        var progress_iter = self.last_progress.iterator();
        while (progress_iter.next()) |entry| {
            self.allocator.free(entry.key_ptr.*);
        }
        self.last_progress.deinit();
        
        self.mutex.unlock();
        
        // Cleanup stream
        self.update_queue.deinit();
        
        self.allocator.destroy(self);
    }
    
    pub fn createTask(self: *ProgressManager, id: []const u8, name: []const u8) !*TaskState {
        const cancel_token = try zsync.future_combinators.createCancelToken(self.allocator);
        const task = try TaskState.init(self.allocator, id, name, cancel_token);
        
        self.mutex.lock();
        defer self.mutex.unlock();
        
        const key = try self.allocator.dupe(u8, id);
        try self.tasks.put(key, task);
        try self.last_progress.put(try self.allocator.dupe(u8, id), 0.0);
        
        _ = self.stats.total_tasks.fetchAdd(1, .acq_rel);
        
        // Send initial progress update
        var initial_update = try ProgressUpdate.init(self.allocator, id, .initializing, 0.0, "Task started");
        defer initial_update.deinit(self.allocator);
        
        try self.sendProgressUpdate(initial_update);
        
        return task;
    }
    
    pub fn updateProgress(
        self: *ProgressManager,
        task_id: []const u8,
        phase: ProgressPhase,
        progress: f32,
        message: []const u8
    ) !void {
        self.mutex.lockShared();
        const task = self.tasks.get(task_id);
        self.mutex.unlockShared();
        
        if (task == null) {
            return ProgressError.TaskNotFound;
        }
        
        const clamped_progress = std.math.clamp(progress, 0.0, 1.0);
        
        // Update task state
        task.?.phase.store(phase, .release);
        task.?.progress.store(clamped_progress, .release);
        task.?.last_update.store(std.time.milliTimestamp(), .release);
        
        // Create progress update
        var update = try ProgressUpdate.init(self.allocator, task_id, phase, clamped_progress, message);
        defer update.deinit(self.allocator);
        
        // Send update
        try self.sendProgressUpdate(update);
        
        // Update statistics
        switch (phase) {
            .completed => _ = self.stats.completed_tasks.fetchAdd(1, .acq_rel),
            .cancelled => _ = self.stats.cancelled_tasks.fetchAdd(1, .acq_rel),
            .failed => _ = self.stats.failed_tasks.fetchAdd(1, .acq_rel),
            else => {},
        }
    }
    
    pub fn updateProgressWithDetails(
        self: *ProgressManager,
        task_id: []const u8,
        phase: ProgressPhase,
        progress: f32,
        message: []const u8,
        details: ProgressUpdate.ProgressDetails
    ) !void {
        var update = try ProgressUpdate.init(self.allocator, task_id, phase, progress, message);
        defer update.deinit(self.allocator);
        
        update.details = details;
        
        try self.sendProgressUpdate(update);
    }
    
    fn sendProgressUpdate(self: *ProgressManager, update: ProgressUpdate) !void {
        // Clone update for async processing
        const cloned_update = try update.clone(self.allocator);
        
        // Send to stream for processing
        try self.update_queue.write(@ptrCast(&cloned_update));
    }
    
    pub fn subscribe(self: *ProgressManager, subscriber: ProgressSubscriber) !void {
        self.mutex.lock();
        defer self.mutex.unlock();
        
        try self.subscribers.append(subscriber);
        _ = self.stats.subscribers_count.fetchAdd(1, .acq_rel);
    }
    
    pub fn unsubscribe(self: *ProgressManager, subscriber_id: []const u8) void {
        self.mutex.lock();
        defer self.mutex.unlock();
        
        for (self.subscribers.items, 0..) |*subscriber, i| {
            if (std.mem.eql(u8, subscriber.id, subscriber_id)) {
                subscriber.deinit(self.allocator);
                _ = self.subscribers.swapRemove(i);
                _ = self.stats.subscribers_count.fetchSub(1, .acq_rel);
                break;
            }
        }
    }
    
    pub fn cancelTask(self: *ProgressManager, task_id: []const u8) !void {
        self.mutex.lockShared();
        defer self.mutex.unlockShared();
        
        if (self.tasks.get(task_id)) |task| {
            task.cancel();
            
            // Send cancellation update
            try self.updateProgress(task_id, .cancelled, task.progress.load(.acquire), "Task cancelled");
            
            // Cancel all subtasks
            for (task.subtasks.items) |subtask_id| {
                self.cancelTask(subtask_id) catch {}; // Continue even if subtask cancellation fails
            }
        }
    }
    
    pub fn getTaskStatus(self: *ProgressManager, task_id: []const u8) ?struct {
        phase: ProgressPhase,
        progress: f32,
        elapsed_time_ms: u64,
        estimated_remaining_ms: ?u64,
    } {
        self.mutex.lockShared();
        defer self.mutex.unlockShared();
        
        if (self.tasks.get(task_id)) |task| {
            return .{
                .phase = task.phase.load(.acquire),
                .progress = task.progress.load(.acquire),
                .elapsed_time_ms = task.getElapsedTime(),
                .estimated_remaining_ms = task.getEstimatedTimeRemaining(),
            };
        }
        
        return null;
    }
    
    pub fn getAllTaskStatuses(self: *ProgressManager) ![]struct {
        id: []const u8,
        name: []const u8,
        phase: ProgressPhase,
        progress: f32,
        elapsed_time_ms: u64,
    } {
        self.mutex.lockShared();
        defer self.mutex.unlockShared();
        
        var statuses = std.ArrayList(@TypeOf(@as([]struct {
            id: []const u8,
            name: []const u8,
            phase: ProgressPhase,
            progress: f32,
            elapsed_time_ms: u64,
        }, undefined)[0])).init(self.allocator);
        defer statuses.deinit();
        
        var task_iter = self.tasks.iterator();
        while (task_iter.next()) |entry| {
            const task = entry.value_ptr.*;
            try statuses.append(.{
                .id = task.id,
                .name = task.name,
                .phase = task.phase.load(.acquire),
                .progress = task.progress.load(.acquire),
                .elapsed_time_ms = task.getElapsedTime(),
            });
        }
        
        return statuses.toOwnedSlice();
    }
    
    pub fn getStatistics(self: *const ProgressManager) struct {
        total_tasks: u64,
        completed_tasks: u64,
        cancelled_tasks: u64,
        failed_tasks: u64,
        active_tasks: u64,
        updates_sent: u64,
        subscribers_count: u32,
    } {
        const total = self.stats.total_tasks.load(.acquire);
        const completed = self.stats.completed_tasks.load(.acquire);
        const cancelled = self.stats.cancelled_tasks.load(.acquire);
        const failed = self.stats.failed_tasks.load(.acquire);
        
        return .{
            .total_tasks = total,
            .completed_tasks = completed,
            .cancelled_tasks = cancelled,
            .failed_tasks = failed,
            .active_tasks = total - completed - cancelled - failed,
            .updates_sent = self.stats.updates_sent.load(.acquire),
            .subscribers_count = self.stats.subscribers_count.load(.acquire),
        };
    }
    
    fn progressWorkerMain(self: *ProgressManager) void {
        std.log.info("Progress manager worker started");
        
        while (!self.shutdown.load(.acquire)) {
            // Read progress updates from stream
            var buffer: [16]u8 = undefined;
            const bytes_read = self.update_queue.read(&buffer) catch |err| switch (err) {
                error.WouldBlock => {
                    std.time.sleep(10 * std.time.ns_per_ms); // 10ms
                    continue;
                },
                error.StreamClosed => break,
                else => {
                    std.log.err("Progress stream read error: {}", .{err});
                    continue;
                },
            };
            
            if (bytes_read == 0) continue;
            
            // Deserialize progress update
            const update_ptr = @as(*ProgressUpdate, @ptrCast(@alignCast(buffer.ptr)));
            defer update_ptr.deinit(self.allocator);
            
            // Get last progress for this task
            self.mutex.lockShared();
            const last_progress = self.last_progress.get(update_ptr.task_id) orelse 0.0;
            self.mutex.unlockShared();
            
            // Notify subscribers
            self.mutex.lockShared();
            for (self.subscribers.items) |*subscriber| {
                if (subscriber.shouldReceiveUpdate(update_ptr, last_progress)) {
                    subscriber.callback(update_ptr.*, subscriber.user_data);
                }
            }
            self.mutex.unlockShared();
            
            // Update last progress
            self.mutex.lock();
            if (self.last_progress.getPtr(update_ptr.task_id)) |last_ptr| {
                last_ptr.* = update_ptr.progress;
            }
            self.mutex.unlock();
            
            _ = self.stats.updates_sent.fetchAdd(1, .acq_rel);
        }
        
        std.log.info("Progress manager worker stopped");
    }
};

// Convenience functions for common progress patterns

pub fn createProgressSubscriber(
    allocator: std.mem.Allocator,
    id: []const u8,
    callback: *const fn(update: ProgressUpdate, user_data: ?*anyopaque) void
) !ProgressSubscriber {
    return ProgressSubscriber.init(allocator, id, callback);
}

pub fn createTaskProgressReporter(
    manager: *ProgressManager,
    task_id: []const u8,
    total_steps: u32
) TaskProgressReporter {
    return TaskProgressReporter{
        .manager = manager,
        .task_id = task_id,
        .total_steps = total_steps,
        .current_step = 0,
    };
}

pub const TaskProgressReporter = struct {
    manager: *ProgressManager,
    task_id: []const u8,
    total_steps: u32,
    current_step: u32,
    
    pub fn nextStep(self: *TaskProgressReporter, message: []const u8) !void {
        self.current_step += 1;
        const progress = @as(f32, @floatFromInt(self.current_step)) / @as(f32, @floatFromInt(self.total_steps));
        
        try self.manager.updateProgress(self.task_id, .building, progress, message);
    }
    
    pub fn setStepProgress(self: *TaskProgressReporter, step_progress: f32, message: []const u8) !void {
        const base_progress = @as(f32, @floatFromInt(self.current_step - 1)) / @as(f32, @floatFromInt(self.total_steps));
        const step_size = 1.0 / @as(f32, @floatFromInt(self.total_steps));
        const total_progress = base_progress + (step_size * step_progress);
        
        try self.manager.updateProgress(self.task_id, .building, total_progress, message);
    }
    
    pub fn complete(self: *TaskProgressReporter, message: []const u8) !void {
        try self.manager.updateProgress(self.task_id, .completed, 1.0, message);
    }
    
    pub fn fail(self: *TaskProgressReporter, error_message: []const u8) !void {
        try self.manager.updateProgress(self.task_id, .failed, 0.0, error_message);
    }
};