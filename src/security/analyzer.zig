const std = @import("std");

pub const SecurityLevel = enum {
    safe,
    warning,
    danger,
    critical,
    
    pub fn toFloat(self: SecurityLevel) f32 {
        return switch (self) {
            .safe => 0.0,
            .warning => -0.5,
            .danger => -1.0,
            .critical => -2.0,
        };
    }
};

pub const SecurityPattern = struct {
    name: []const u8,
    pattern: []const u8,
    description: []const u8,
    level: SecurityLevel,
    is_regex: bool,
};

pub const SecurityViolation = struct {
    pattern: *const SecurityPattern,
    line_number: u32,
    line_content: []const u8,
    context: []const u8,
};

pub const SecurityReport = struct {
    violations: []SecurityViolation,
    overall_score: f32,
    risk_level: SecurityLevel,
    summary: []const u8,
    
    pub fn deinit(self: *SecurityReport, allocator: std.mem.Allocator) void {
        for (self.violations) |violation| {
            allocator.free(violation.line_content);
            allocator.free(violation.context);
        }
        allocator.free(self.violations);
        allocator.free(self.summary);
    }
};

pub const PkgbuildAnalyzer = struct {
    allocator: std.mem.Allocator,
    security_patterns: []const SecurityPattern,
    
    pub fn init(allocator: std.mem.Allocator) !*PkgbuildAnalyzer {
        const self = try allocator.create(PkgbuildAnalyzer);
        self.* = .{
            .allocator = allocator,
            .security_patterns = &security_patterns,
        };
        return self;
    }
    
    pub fn deinit(self: *PkgbuildAnalyzer) void {
        self.allocator.destroy(self);
    }
    
    pub fn analyzeFile(self: *PkgbuildAnalyzer, file_path: []const u8) !SecurityReport {
        const file = try std.fs.openFileAbsolute(file_path, .{});
        defer file.close();
        
        const file_size = try file.getEndPos();
        if (file_size > 1024 * 1024) return error.FileTooLarge;
        const content = try self.allocator.alloc(u8, file_size);
        _ = try file.readAll(content); // 1MB limit
        defer self.allocator.free(content);
        
        return self.analyzeContent(content);
    }
    
    pub fn analyzeContent(self: *PkgbuildAnalyzer, content: []const u8) !SecurityReport {
        var violations = std.ArrayList(SecurityViolation){};
        var line_number: u32 = 1;
        var lines = std.mem.tokenizeScalar(u8, content, '\n');
        
        while (lines.next()) |line| {
            defer line_number += 1;
            
            // Check each security pattern against this line
            for (self.security_patterns) |*pattern| {
                if (self.matchesPattern(line, pattern)) {
                    const violation = SecurityViolation{
                        .pattern = pattern,
                        .line_number = line_number,
                        .line_content = try self.allocator.dupe(u8, line),
                        .context = try self.extractContext(content, line_number),
                    };
                    try violations.append(self.allocator, violation);
                }
            }
        }
        
        const violations_slice = try violations.toOwnedSlice(self.allocator);
        const overall_score = self.calculateOverallScore(violations_slice);
        const risk_level = self.determineRiskLevel(violations_slice, overall_score);
        const summary = try self.generateSummary(violations_slice, overall_score);
        
        return SecurityReport{
            .violations = violations_slice,
            .overall_score = overall_score,
            .risk_level = risk_level,
            .summary = summary,
        };
    }
    
    pub fn scanForSecurity(self: *PkgbuildAnalyzer, pkgbuild_content: []const u8) !SecurityReport {
        return self.analyzeContent(pkgbuild_content);
    }
    
    pub fn detectNetworkAccess(content: []const u8) bool {
        const network_patterns = [_][]const u8{
            "curl", "wget", "git clone", "svn", "hg clone",
            "http://", "https://", "ftp://", "ftps://",
            "rsync", "scp", "ssh", "telnet",
        };
        
        for (network_patterns) |pattern| {
            if (std.mem.indexOf(u8, content, pattern) != null) {
                return true;
            }
        }
        return false;
    }
    
    pub fn detectPrivilegeEscalation(content: []const u8) bool {
        const privilege_patterns = [_][]const u8{
            "sudo", "su -", "setuid", "setgid", "chmod +s",
            "/etc/passwd", "/etc/shadow", "/etc/sudoers",
            "SUID", "SGID", "sticky bit",
        };
        
        for (privilege_patterns) |pattern| {
            if (std.mem.indexOf(u8, content, pattern) != null) {
                return true;
            }
        }
        return false;
    }
    
    pub fn scanForCredentials(self: *PkgbuildAnalyzer, content: []const u8) ![]CredentialPattern {
        var credentials = std.ArrayList(CredentialPattern){};
        
        const credential_patterns = [_]CredentialPattern{
            .{ .pattern = "password=", .type = .password, .severity = .critical },
            .{ .pattern = "passwd=", .type = .password, .severity = .critical },
            .{ .pattern = "token=", .type = .api_token, .severity = .danger },
            .{ .pattern = "api_key=", .type = .api_key, .severity = .danger },
            .{ .pattern = "secret=", .type = .secret, .severity = .danger },
            .{ .pattern = "private_key", .type = .private_key, .severity = .critical },
            .{ .pattern = "-----BEGIN", .type = .certificate, .severity = .warning },
        };
        
        for (credential_patterns) |pattern| {
            if (std.mem.indexOf(u8, content, pattern.pattern) != null) {
                try credentials.append(self.allocator, pattern);
            }
        }
        
        return credentials.toOwnedSlice(self.allocator);
    }
    
    fn matchesPattern(self: *PkgbuildAnalyzer, line: []const u8, pattern: *const SecurityPattern) bool {
        _ = self;
        
        if (pattern.is_regex) {
            // TODO: Implement regex matching when available
            return std.mem.indexOf(u8, line, pattern.pattern) != null;
        } else {
            return std.mem.indexOf(u8, line, pattern.pattern) != null;
        }
    }
    
    fn extractContext(self: *PkgbuildAnalyzer, content: []const u8, target_line: u32) ![]const u8 {
        const context_lines = 2;
        var lines = std.mem.tokenizeScalar(u8, content, '\n');
        var current_line: u32 = 1;
        var context = std.ArrayList(u8){};
        
        // Find target line and extract context
        while (lines.next()) |line| {
            defer current_line += 1;
            
            if (current_line >= target_line - context_lines and current_line <= target_line + context_lines) {
                try context.appendSlice(self.allocator, line);
                try context.append(self.allocator, '\n');
            }
            
            if (current_line > target_line + context_lines) break;
        }
        
        return context.toOwnedSlice(self.allocator);
    }
    
    fn calculateOverallScore(self: *PkgbuildAnalyzer, violations: []SecurityViolation) f32 {
        _ = self;
        var score: f32 = 0.0;
        
        for (violations) |violation| {
            score += violation.pattern.level.toFloat();
        }
        
        return score;
    }
    
    fn determineRiskLevel(self: *PkgbuildAnalyzer, violations: []SecurityViolation, score: f32) SecurityLevel {
        _ = self;
        _ = violations;
        
        if (score <= -3.0) return .critical;
        if (score <= -1.5) return .danger;
        if (score <= -0.5) return .warning;
        return .safe;
    }
    
    fn generateSummary(self: *PkgbuildAnalyzer, violations: []SecurityViolation, score: f32) ![]const u8 {
        if (violations.len == 0) {
            return try self.allocator.dupe(u8, "No security violations detected. This PKGBUILD appears safe.");
        }
        
        var critical_count: u32 = 0;
        var danger_count: u32 = 0;
        var warning_count: u32 = 0;
        
        for (violations) |violation| {
            switch (violation.pattern.level) {
                .critical => critical_count += 1,
                .danger => danger_count += 1,
                .warning => warning_count += 1,
                .safe => {},
            }
        }
        
        var summary = std.ArrayList(u8){};
        try summary.appendSlice(self.allocator, "Security analysis found ");
        try summary.appendSlice(self.allocator, try std.fmt.allocPrint(self.allocator, "{d}", .{violations.len}));
        try summary.appendSlice(self.allocator, " potential issues (Score: ");
        try summary.appendSlice(self.allocator, try std.fmt.allocPrint(self.allocator, "{d:.1}", .{score}));
        try summary.appendSlice(self.allocator, "). ");
        
        if (critical_count > 0) {
            try summary.appendSlice(self.allocator, try std.fmt.allocPrint(self.allocator, "{d} critical, ", .{critical_count}));
        }
        if (danger_count > 0) {
            try summary.appendSlice(self.allocator, try std.fmt.allocPrint(self.allocator, "{d} dangerous, ", .{danger_count}));
        }
        if (warning_count > 0) {
            try summary.appendSlice(self.allocator, try std.fmt.allocPrint(self.allocator, "{d} warnings.", .{warning_count}));
        }
        
        return summary.toOwnedSlice(self.allocator);
    }
};

pub const CredentialType = enum {
    password,
    api_token,
    api_key,
    secret,
    private_key,
    certificate,
};

pub const CredentialPattern = struct {
    pattern: []const u8,
    type: CredentialType,
    severity: SecurityLevel,
};

// Comprehensive security patterns inspired by the Rust implementation
const security_patterns = [_]SecurityPattern{
    // Network access patterns
    .{ .name = "curl_download", .pattern = "curl", .description = "Downloads files from internet", .level = .warning, .is_regex = false },
    .{ .name = "wget_download", .pattern = "wget", .description = "Downloads files from internet", .level = .warning, .is_regex = false },
    .{ .name = "git_clone", .pattern = "git clone", .description = "Clones Git repository", .level = .warning, .is_regex = false },
    .{ .name = "http_url", .pattern = "http://", .description = "Insecure HTTP connection", .level = .danger, .is_regex = false },
    
    // Privilege escalation
    .{ .name = "sudo_usage", .pattern = "sudo", .description = "Requires root privileges", .level = .warning, .is_regex = false },
    .{ .name = "setuid", .pattern = "setuid", .description = "Sets user ID on execution", .level = .danger, .is_regex = false },
    .{ .name = "chmod_suid", .pattern = "chmod +s", .description = "Sets SUID/SGID bit", .level = .critical, .is_regex = false },
    
    // System file access
    .{ .name = "passwd_file", .pattern = "/etc/passwd", .description = "Accesses password file", .level = .critical, .is_regex = false },
    .{ .name = "shadow_file", .pattern = "/etc/shadow", .description = "Accesses shadow password file", .level = .critical, .is_regex = false },
    .{ .name = "sudoers_file", .pattern = "/etc/sudoers", .description = "Modifies sudo configuration", .level = .critical, .is_regex = false },
    
    // Credential patterns
    .{ .name = "hardcoded_password", .pattern = "password=", .description = "Hardcoded password detected", .level = .critical, .is_regex = false },
    .{ .name = "api_token", .pattern = "token=", .description = "API token found", .level = .danger, .is_regex = false },
    .{ .name = "api_key", .pattern = "api_key=", .description = "API key found", .level = .danger, .is_regex = false },
    .{ .name = "secret_key", .pattern = "secret=", .description = "Secret key found", .level = .danger, .is_regex = false },
    
    // Suspicious commands
    .{ .name = "rm_rf", .pattern = "rm -rf", .description = "Recursive file deletion", .level = .warning, .is_regex = false },
    .{ .name = "dd_command", .pattern = "dd if=", .description = "Low-level disk operations", .level = .danger, .is_regex = false },
    .{ .name = "eval_command", .pattern = "eval", .description = "Dynamic code execution", .level = .danger, .is_regex = false },
    .{ .name = "base64_decode", .pattern = "base64 -d", .description = "Base64 decoding (potential obfuscation)", .level = .warning, .is_regex = false },
    
    // Compiler/build bypasses
    .{ .name = "skip_checksum", .pattern = "SKIP", .description = "Checksum verification skipped", .level = .danger, .is_regex = false },
    .{ .name = "no_check", .pattern = "--no-check", .description = "Build checks disabled", .level = .warning, .is_regex = false },
    .{ .name = "force_install", .pattern = "--force", .description = "Forced installation", .level = .warning, .is_regex = false },
    
    // Network listeners
    .{ .name = "netcat_listen", .pattern = "nc -l", .description = "Network listener (potential backdoor)", .level = .critical, .is_regex = false },
    .{ .name = "socat_listener", .pattern = "socat", .description = "Advanced network tools", .level = .warning, .is_regex = false },
    
    // Package manager abuse
    .{ .name = "pip_install", .pattern = "pip install", .description = "Python package installation", .level = .warning, .is_regex = false },
    .{ .name = "npm_install", .pattern = "npm install", .description = "Node.js package installation", .level = .warning, .is_regex = false },
    .{ .name = "gem_install", .pattern = "gem install", .description = "Ruby gem installation", .level = .warning, .is_regex = false },
    
    // Binary execution from internet
    .{ .name = "curl_bash", .pattern = "curl | bash", .description = "Executes downloaded script", .level = .critical, .is_regex = false },
    .{ .name = "wget_bash", .pattern = "wget | bash", .description = "Executes downloaded script", .level = .critical, .is_regex = false },
    
    // Systemd service manipulation
    .{ .name = "systemctl_enable", .pattern = "systemctl enable", .description = "Enables system service", .level = .warning, .is_regex = false },
    .{ .name = "systemd_unit", .pattern = ".service", .description = "Systemd service file", .level = .warning, .is_regex = false },
    
    // Cron job installation
    .{ .name = "crontab", .pattern = "crontab", .description = "Scheduled task installation", .level = .warning, .is_regex = false },
    .{ .name = "cron_dir", .pattern = "/etc/cron", .description = "Cron directory access", .level = .warning, .is_regex = false },
};