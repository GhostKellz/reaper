const std = @import("std");
const zsync = @import("zsync");

pub const SecurityError = error{
    SuspiciousCode,
    MaliciousPattern,
    NetworkDownload,
    PrivilegeEscalation,
    UnverifiedSource,
    UnsafeExecution,
    FileSystemAccess,
    SystemModification,
} || std.mem.Allocator.Error;

pub const SecurityLevel = enum(u8) {
    safe = 0,
    low_risk = 1,
    medium_risk = 2,
    high_risk = 3,
    dangerous = 4,
};

pub const SecurityViolation = struct {
    rule_id: []const u8,
    severity: SecurityLevel,
    category: ViolationCategory,
    description: []const u8,
    line_number: u32,
    code_snippet: []const u8,
    recommendation: []const u8,
    auto_fixable: bool,
    
    const ViolationCategory = enum {
        network_access,
        file_system,
        privilege_escalation, 
        code_injection,
        data_exfiltration,
        malicious_payload,
        suspicious_behavior,
        unsafe_practice,
        system_modification,
    };
    
    pub fn init(allocator: std.mem.Allocator, rule_id: []const u8, severity: SecurityLevel, category: ViolationCategory, description: []const u8) !SecurityViolation {
        return SecurityViolation{
            .rule_id = try allocator.dupe(u8, rule_id),
            .severity = severity,
            .category = category,
            .description = try allocator.dupe(u8, description),
            .line_number = 0,
            .code_snippet = try allocator.dupe(u8, ""),
            .recommendation = try allocator.dupe(u8, ""),
            .auto_fixable = false,
        };
    }
    
    pub fn deinit(self: *SecurityViolation, allocator: std.mem.Allocator) void {
        allocator.free(self.rule_id);
        allocator.free(self.description);
        allocator.free(self.code_snippet);
        allocator.free(self.recommendation);
    }
};

pub const SecurityRule = struct {
    id: []const u8,
    name: []const u8,
    pattern: []const u8,
    severity: SecurityLevel,
    category: SecurityViolation.ViolationCategory,
    description: []const u8,
    recommendation: []const u8,
    enabled: bool,
    regex_compiled: ?bool, // Simplified for now - would use regex in production
    
    pub fn init(allocator: std.mem.Allocator, id: []const u8, name: []const u8, pattern: []const u8, severity: SecurityLevel, category: SecurityViolation.ViolationCategory) !SecurityRule {
        return SecurityRule{
            .id = try allocator.dupe(u8, id),
            .name = try allocator.dupe(u8, name),
            .pattern = try allocator.dupe(u8, pattern),
            .severity = severity,
            .category = category,
            .description = try allocator.dupe(u8, ""),
            .recommendation = try allocator.dupe(u8, ""),
            .enabled = true,
            .regex_compiled = null,
        };
    }
    
    pub fn deinit(self: *SecurityRule, allocator: std.mem.Allocator) void {
        allocator.free(self.id);
        allocator.free(self.name);
        allocator.free(self.pattern);
        allocator.free(self.description);
        allocator.free(self.recommendation);
        // No cleanup needed for simplified regex_compiled
    }
};

pub const SecurityScanResult = struct {
    file_path: []const u8,
    violations: []SecurityViolation,
    overall_risk: SecurityLevel,
    scan_time_ms: u64,
    lines_analyzed: u32,
    rules_applied: u32,
    safe_for_install: bool,
    warnings: [][]const u8,
    allocator: std.mem.Allocator,
    
    pub fn deinit(self: *SecurityScanResult) void {
        self.allocator.free(self.file_path);
        
        for (self.violations) |*violation| {
            violation.deinit(self.allocator);
        }
        self.allocator.free(self.violations);
        
        for (self.warnings) |warning| {
            self.allocator.free(warning);
        }
        self.allocator.free(self.warnings);
    }
    
    pub fn calculateRisk(self: *const SecurityScanResult) SecurityLevel {
        var max_risk: u8 = 0;
        for (self.violations) |violation| {
            max_risk = @max(max_risk, @intFromEnum(violation.severity));
        }
        return @enumFromInt(max_risk);
    }
};

pub const SecurityRuleEngine = struct {
    allocator: std.mem.Allocator,
    rules: std.ArrayList(SecurityRule),
    custom_rules: std.StringHashMap(SecurityRule),
    enabled_categories: std.EnumSet(SecurityViolation.ViolationCategory),
    
    pub fn init(allocator: std.mem.Allocator) !*SecurityRuleEngine {
        const engine = try allocator.create(SecurityRuleEngine);
        engine.* = SecurityRuleEngine{
            .allocator = allocator,
            .rules = std.ArrayList(SecurityRule){},
            .custom_rules = std.StringHashMap(SecurityRule).init(allocator),
            .enabled_categories = std.EnumSet(SecurityViolation.ViolationCategory).initFull(),
        };
        
        // Load default security rules
        try engine.loadDefaultRules();
        
        return engine;
    }
    
    pub fn deinit(self: *SecurityRuleEngine) void {
        for (self.rules.items) |*rule| {
            rule.deinit(self.allocator);
        }
        self.rules.deinit(self.allocator);
        
        var custom_iter = self.custom_rules.iterator();
        while (custom_iter.next()) |entry| {
            entry.value_ptr.deinit(self.allocator);
            self.allocator.free(entry.key_ptr.*);
        }
        self.custom_rules.deinit();
        
        self.allocator.destroy(self);
    }
    
    fn loadDefaultRules(self: *SecurityRuleEngine) !void {
        // Network access patterns
        try self.addRule("NET001", "Suspicious curl usage", 
            "curl.*\\|.*sh|curl.*\\|.*bash", .high_risk, .network_access);
        
        try self.addRule("NET002", "wget pipe to shell", 
            "wget.*\\|.*sh|wget.*\\|.*bash", .high_risk, .network_access);
        
        try self.addRule("NET003", "Unverified HTTPS disable",
            "--insecure|--no-check-certificate", .medium_risk, .unsafe_practice);
        
        try self.addRule("NET004", "HTTP instead of HTTPS", 
            "http://(?!localhost|127\\.0\\.0\\.1)", .low_risk, .unsafe_practice);
        
        // File system access
        try self.addRule("FS001", "System directory modification",
            "/etc/|/usr/bin/|/bin/|/sbin/|/usr/sbin/", .high_risk, .file_system);
            
        try self.addRule("FS002", "Hidden file creation",
            "touch.*\\.|mkdir.*\\.|\\. [a-zA-Z]", .medium_risk, .suspicious_behavior);
            
        try self.addRule("FS003", "Chmod 777 usage",
            "chmod.*777|chmod.*a\\+rwx", .medium_risk, .unsafe_practice);
        
        // Privilege escalation
        try self.addRule("PRIV001", "Sudo without password",
            "sudo.*NOPASSWD|sudoers.*NOPASSWD", .high_risk, .privilege_escalation);
            
        try self.addRule("PRIV002", "SUID bit setting",
            "chmod.*[4567][0-9][0-9][0-9]|chmod.*\\+s", .high_risk, .privilege_escalation);
        
        // Code injection
        try self.addRule("INJ001", "Shell command injection",
            "\\$\\([^)]*\\)|`[^`]*`|eval.*\\$", .high_risk, .code_injection);
            
        try self.addRule("INJ002", "Dangerous variable expansion",
            "\\$\\{[^}]*\\}.*sh|\\$\\{[^}]*\\}.*bash", .medium_risk, .code_injection);
        
        // Malicious patterns  
        try self.addRule("MAL001", "Base64 encoded content",
            "base64.*-d|echo.*\\|.*base64", .medium_risk, .malicious_payload);
            
        try self.addRule("MAL002", "Hex encoded content",
            "xxd.*-r.*-p|echo.*\\|.*xxd", .medium_risk, .malicious_payload);
            
        try self.addRule("MAL003", "Obfuscated scripts",
            "\\$'\\\\[0-9]+'|\\\\x[0-9a-fA-F]{2}", .high_risk, .malicious_payload);
        
        // Data exfiltration
        try self.addRule("EXFIL001", "Network data transmission",
            "nc.*-e|netcat.*-e|/dev/tcp/", .high_risk, .data_exfiltration);
            
        try self.addRule("EXFIL002", "File upload patterns",
            "scp.*\\$|rsync.*\\$|curl.*-T", .medium_risk, .data_exfiltration);
        
        // Suspicious behavior
        try self.addRule("SUSP001", "Process backgrounding",
            "&[[:space:]]*$|nohup.*&", .low_risk, .suspicious_behavior);
            
        try self.addRule("SUSP002", "Output redirection to dev/null",
            ">/dev/null.*2>&1|2>/dev/null.*>/dev/null", .low_risk, .suspicious_behavior);
            
        try self.addRule("SUSP003", "Cron job modification",
            "crontab.*-e|echo.*crontab|\\*/etc/cron", .medium_risk, .system_modification);
    }
    
    fn addRule(self: *SecurityRuleEngine, id: []const u8, name: []const u8, pattern: []const u8, severity: SecurityLevel, category: SecurityViolation.ViolationCategory) !void {
        const rule = try SecurityRule.init(self.allocator, id, name, pattern, severity, category);
        try self.rules.append(self.allocator, rule);
    }
};

pub const ConcurrentSecurityScanner = struct {
    allocator: std.mem.Allocator,
    rule_engine: *SecurityRuleEngine,
    scan_queue: std.ArrayList(ScanTask), // Simplified queue for now
    worker_threads: []std.Thread,
    results: std.StringHashMap(SecurityScanResult),
    mutex: std.Thread.RwLock,
    shutdown: std.atomic.Value(bool),
    stats: ScanStats,
    
    const ScanTask = struct {
        id: []const u8,
        file_path: []const u8,
        content: []const u8,
        priority: u8,
        callback: ?*const fn(result: SecurityScanResult) void,
    };
    
    const ScanStats = struct {
        files_scanned: std.atomic.Value(u64),
        violations_found: std.atomic.Value(u64),
        high_risk_files: std.atomic.Value(u32),
        scan_time_total_ms: std.atomic.Value(u64),
        concurrent_scans: std.atomic.Value(u32),
    };
    
    pub fn init(allocator: std.mem.Allocator, max_workers: u8) !*ConcurrentSecurityScanner {
        const scanner = try allocator.create(ConcurrentSecurityScanner);
        
        scanner.* = ConcurrentSecurityScanner{
            .allocator = allocator,
            .rule_engine = try SecurityRuleEngine.init(allocator),
            .scan_queue = std.ArrayList(ScanTask){},
            .worker_threads = try allocator.alloc(std.Thread, max_workers),
            .results = std.StringHashMap(SecurityScanResult).init(allocator),
            .mutex = std.Thread.RwLock{},
            .shutdown = std.atomic.Value(bool).init(false),
            .stats = ScanStats{
                .files_scanned = std.atomic.Value(u64).init(0),
                .violations_found = std.atomic.Value(u64).init(0),
                .high_risk_files = std.atomic.Value(u32).init(0),
                .scan_time_total_ms = std.atomic.Value(u64).init(0),
                .concurrent_scans = std.atomic.Value(u32).init(0),
            },
        };
        
        // Start worker threads
        for (scanner.worker_threads, 0..) |*thread, i| {
            thread.* = try std.Thread.spawn(.{}, scanWorkerMain, .{ scanner, i });
        }
        
        return scanner;
    }
    
    pub fn deinit(self: *ConcurrentSecurityScanner) void {
        // Signal shutdown
        self.shutdown.store(true, .release);
        
        // Wait for workers
        for (self.worker_threads) |thread| {
            thread.join();
        }
        
        // Cleanup
        self.allocator.free(self.worker_threads);
        self.scan_queue.deinit(self.allocator);
        self.rule_engine.deinit();
        
        // Cleanup results
        self.mutex.lock();
        var result_iter = self.results.iterator();
        while (result_iter.next()) |entry| {
            entry.value_ptr.deinit();
            self.allocator.free(entry.key_ptr.*);
        }
        self.results.deinit();
        self.mutex.unlock();
        
        self.allocator.destroy(self);
    }
    
    pub fn scanPkgbuildAsync(self: *ConcurrentSecurityScanner, file_path: []const u8, priority: u8) ![]const u8 {
        // Generate task ID
        const task_id = try std.fmt.allocPrint(self.allocator, "scan_{d}_{s}", .{ std.time.timestamp(), std.fs.path.basename(file_path) });
        
        // Read file content
        const file = try std.fs.openFileAbsolute(file_path, .{});
        defer file.close();
        
        const file_size = try file.getEndPos();
        if (file_size > 1024 * 1024) return error.FileTooLarge;
        const content = try self.allocator.alloc(u8, file_size);
        _ = try file.readAll(content); // 1MB limit
        
        // Create scan task
        const task = ScanTask{
            .id = task_id,
            .file_path = try self.allocator.dupe(u8, file_path),
            .content = content,
            .priority = priority,
            .callback = null,
        };
        
        // Enqueue task (simplified - would use proper concurrent queue)
        self.mutex.lock();
        try self.scan_queue.append(self.allocator, task);
        self.mutex.unlock();
        
        return task_id;
    }
    
    pub fn scanPkgbuildSync(self: *ConcurrentSecurityScanner, file_path: []const u8) !SecurityScanResult {
        const file = try std.fs.openFileAbsolute(file_path, .{});
        defer file.close();
        
        const file_size = try file.getEndPos();
        if (file_size > 1024 * 1024) return error.FileTooLarge;
        const content = try self.allocator.alloc(u8, file_size);
        _ = try file.readAll(content);
        defer self.allocator.free(content);
        
        return try self.analyzeContent(file_path, content);
    }
    
    pub fn getResult(self: *ConcurrentSecurityScanner, task_id: []const u8) ?SecurityScanResult {
        self.mutex.lockShared();
        defer self.mutex.unlockShared();
        
        return self.results.get(task_id);
    }
    
    pub fn waitForResult(self: *ConcurrentSecurityScanner, task_id: []const u8, timeout_ms: u64) ?SecurityScanResult {
        const deadline = std.time.milliTimestamp() + @as(i64, @intCast(timeout_ms));
        
        while (std.time.milliTimestamp() < deadline) {
            if (self.getResult(task_id)) |result| {
                return result;
            }
            std.Thread.sleep(10 * std.time.ns_per_ms);
        }
        
        return null;
    }
    
    fn scanWorkerMain(self: *ConcurrentSecurityScanner, worker_id: usize) void {
        std.log.info("Security scanner worker {} started", .{worker_id});
        
        while (!self.shutdown.load(.acquire)) {
            // Check for tasks (simplified)
            self.mutex.lock();
            const maybe_task = if (self.scan_queue.items.len > 0) self.scan_queue.pop() else null;
            self.mutex.unlock();
            
            const task = maybe_task orelse {
                std.Thread.sleep(10 * std.time.ns_per_ms);
                continue;
            };
            
            _ = self.stats.concurrent_scans.fetchAdd(1, .acq_rel);
            defer _ = self.stats.concurrent_scans.fetchSub(1, .acq_rel);
            
            // Analyze content
            const result = self.analyzeContent(task.file_path, task.content) catch |err| {
                std.log.err("Security analysis failed for {s}: {}", .{ task.file_path, err });
                continue;
            };
            
            // Store result
            self.mutex.lock();
            const key = self.allocator.dupe(u8, task.id) catch |err| {
                std.log.err("Failed to store scan result: {}", .{err});
                continue;
            };
            self.results.put(key, result) catch |err| {
                std.log.err("Failed to store scan result: {}", .{err});
                self.allocator.free(key);
                continue;
            };
            self.mutex.unlock();
            
            // Update statistics
            _ = self.stats.files_scanned.fetchAdd(1, .acq_rel);
            _ = self.stats.violations_found.fetchAdd(result.violations.len, .acq_rel);
            if (@intFromEnum(result.overall_risk) >= @intFromEnum(SecurityLevel.high_risk)) {
                _ = self.stats.high_risk_files.fetchAdd(1, .acq_rel);
            }
            _ = self.stats.scan_time_total_ms.fetchAdd(result.scan_time_ms, .acq_rel);
            
            // Call callback if provided
            if (task.callback) |callback| {
                callback(result);
            }
            
            // Cleanup task
            self.allocator.free(task.id);
            self.allocator.free(task.file_path);
            self.allocator.free(task.content);
        }
        
        std.log.info("Security scanner worker {} stopped", .{worker_id});
    }
    
    fn analyzeContent(self: *ConcurrentSecurityScanner, file_path: []const u8, content: []const u8) !SecurityScanResult {
        const start_time = std.time.milliTimestamp();
        
        var violations = std.ArrayList(SecurityViolation){};
        defer violations.deinit(self.allocator);
        
        var warnings = std.ArrayList([]const u8){};
        defer warnings.deinit(self.allocator);
        
        // Split content into lines for analysis
        var line_iter = std.mem.splitScalar(u8, content, '\n');
        var line_number: u32 = 0;
        var rules_applied: u32 = 0;
        
        while (line_iter.next()) |line| {
            line_number += 1;
            
            // Apply each security rule
            for (self.rule_engine.rules.items) |rule| {
                if (!rule.enabled) continue;
                
                rules_applied += 1;
                
                if (try self.matchesPattern(line, rule.pattern)) {
                    var violation = try SecurityViolation.init(
                        self.allocator, 
                        rule.id, 
                        rule.severity, 
                        rule.category,
                        rule.description
                    );
                    violation.line_number = line_number;
                    violation.code_snippet = try self.allocator.dupe(u8, std.mem.trim(u8, line, " \t"));
                    violation.recommendation = try self.allocator.dupe(u8, rule.recommendation);
                    
                    try violations.append(self.allocator, violation);
                }
            }
            
            // Additional heuristic analysis
            try self.applyHeuristicAnalysis(line, line_number, &violations, &warnings);
        }
        
        // Calculate overall risk
        var max_risk: u8 = 0;
        for (violations.items) |violation| {
            max_risk = @max(max_risk, @intFromEnum(violation.severity));
        }
        
        const overall_risk: SecurityLevel = @enumFromInt(max_risk);
        const safe_for_install = @intFromEnum(overall_risk) < @intFromEnum(SecurityLevel.high_risk);
        
        const scan_time = @as(u64, @intCast(std.time.milliTimestamp() - start_time));
        
        return SecurityScanResult{
            .file_path = try self.allocator.dupe(u8, file_path),
            .violations = try violations.toOwnedSlice(self.allocator),
            .overall_risk = overall_risk,
            .scan_time_ms = scan_time,
            .lines_analyzed = line_number,
            .rules_applied = rules_applied,
            .safe_for_install = safe_for_install,
            .warnings = try warnings.toOwnedSlice(self.allocator),
            .allocator = self.allocator,
        };
    }
    
    fn matchesPattern(self: *ConcurrentSecurityScanner, text: []const u8, pattern: []const u8) !bool {
        _ = self;
        
        // Simple pattern matching - in production would use proper regex
        // This is a simplified implementation using string contains
        if (std.mem.indexOf(u8, text, pattern)) |_| {
            return true;
        }
        
        // Check for regex-like patterns (simplified)
        if (std.mem.containsAtLeast(u8, pattern, 1, "|")) {
            // OR pattern
            var or_iter = std.mem.splitScalar(u8, pattern, '|');
            while (or_iter.next()) |sub_pattern| {
                const trimmed = std.mem.trim(u8, sub_pattern, " ");
                if (std.mem.indexOf(u8, text, trimmed)) |_| {
                    return true;
                }
            }
        }
        
        return false;
    }
    
    fn applyHeuristicAnalysis(
        self: *ConcurrentSecurityScanner,
        line: []const u8,
        line_number: u32,
        violations: *std.ArrayList(SecurityViolation),
        warnings: *std.ArrayList([]const u8)
    ) !void {
        _ = line_number;
        
        // Entropy analysis for potential encoded content
        if (try self.calculateEntropy(line) > 4.5 and line.len > 50) {
            var violation = try SecurityViolation.init(
                self.allocator,
                "HEUR001",
                .medium_risk,
                .suspicious_behavior,
                "High entropy content detected (possible encoded data)"
            );
            violation.code_snippet = try self.allocator.dupe(u8, line[0..@min(line.len, 100)]);
            violation.recommendation = try self.allocator.dupe(u8, "Review for potential obfuscation or encoded malicious content");
            
            try violations.append(self.allocator, violation);
        }
        
        // URL reputation checking (simplified)
        if (std.mem.indexOf(u8, line, "http") != null) {
            try warnings.append(self.allocator, try self.allocator.dupe(u8, "Network access detected - verify URL reputation"));
        }
        
        // Suspicious command combinations
        if (self.containsSuspiciousCombination(line)) {
            var violation = try SecurityViolation.init(
                self.allocator,
                "HEUR002", 
                .high_risk,
                .suspicious_behavior,
                "Suspicious command combination detected"
            );
            violation.code_snippet = try self.allocator.dupe(u8, line);
            violation.recommendation = try self.allocator.dupe(u8, "Manual review required for command sequence");
            
            try violations.append(self.allocator, violation);
        }
    }
    
    fn calculateEntropy(self: *ConcurrentSecurityScanner, data: []const u8) !f64 {
        _ = self;
        
        if (data.len == 0) return 0.0;
        
        var frequencies = [_]u32{0} ** 256;
        for (data) |byte| {
            frequencies[byte] += 1;
        }
        
        var entropy: f64 = 0.0;
        const length = @as(f64, @floatFromInt(data.len));
        
        for (frequencies) |freq| {
            if (freq > 0) {
                const probability = @as(f64, @floatFromInt(freq)) / length;
                entropy -= probability * @log(probability) / @log(2.0);
            }
        }
        
        return entropy;
    }
    
    fn containsSuspiciousCombination(self: *ConcurrentSecurityScanner, line: []const u8) bool {
        _ = self;
        
        // Common malicious patterns
        const suspicious_patterns = [_][]const u8{
            "curl | sh",
            "wget | bash",
            "curl | bash",
            "eval $(curl",
            "$(wget -qO-",
            "; rm -rf",
            "&& rm -rf /",
        };
        
        for (suspicious_patterns) |pattern| {
            if (std.mem.indexOf(u8, line, pattern) != null) {
                return true;
            }
        }
        
        return false;
    }
    
    pub fn getStatistics(self: *const ConcurrentSecurityScanner) struct {
        files_scanned: u64,
        violations_found: u64,
        high_risk_files: u32,
        average_scan_time_ms: u64,
        concurrent_scans: u32,
        total_rules: usize,
    } {
        const scanned = self.stats.files_scanned.load(.acquire);
        const total_time = self.stats.scan_time_total_ms.load(.acquire);
        
        return .{
            .files_scanned = scanned,
            .violations_found = self.stats.violations_found.load(.acquire),
            .high_risk_files = self.stats.high_risk_files.load(.acquire),
            .average_scan_time_ms = if (scanned > 0) total_time / scanned else 0,
            .concurrent_scans = self.stats.concurrent_scans.load(.acquire),
            .total_rules = self.rule_engine.rules.items.len,
        };
    }
    
    // Batch scanning for multiple files
    pub fn scanBatch(self: *ConcurrentSecurityScanner, file_paths: []const []const u8) ![][]const u8 {
        var task_ids = try self.allocator.alloc([]const u8, file_paths.len);
        
        for (file_paths, 0..) |path, i| {
            task_ids[i] = try self.scanPkgbuildAsync(path, 1);
        }
        
        return task_ids;
    }
    
    // Wait for batch completion
    pub fn waitForBatch(self: *ConcurrentSecurityScanner, task_ids: []const []const u8, timeout_ms: u64) ![]SecurityScanResult {
        var results = try self.allocator.alloc(SecurityScanResult, task_ids.len);
        
        for (task_ids, 0..) |task_id, i| {
            if (self.waitForResult(task_id, timeout_ms)) |result| {
                results[i] = result;
            } else {
                return SecurityError.TimeoutError;
            }
        }
        
        return results;
    }
};

// Convenience functions for common scanning patterns

pub fn scanPkgbuildQuick(allocator: std.mem.Allocator, file_path: []const u8) !SecurityScanResult {
    var scanner = try ConcurrentSecurityScanner.init(allocator, 1);
    defer scanner.deinit();
    
    return try scanner.scanPkgbuildSync(file_path);
}

pub fn isSafeToInstall(allocator: std.mem.Allocator, file_path: []const u8) !bool {
    const result = try scanPkgbuildQuick(allocator, file_path);
    defer result.deinit();
    
    return result.safe_for_install;
}