const std = @import("std");
const gpg = @import("gpg.zig");
const analyzer = @import("analyzer.zig");
const Package = @import("../core/package.zig").Package;

pub const SecurityManager = struct {
    allocator: std.mem.Allocator,
    gpg_manager: *gpg.GpgManager,
    pkgbuild_analyzer: *analyzer.PkgbuildAnalyzer,
    enforce_gpg: bool,
    min_trust_score: f32,
    auto_import_keys: bool,
    
    pub fn init(allocator: std.mem.Allocator) !*SecurityManager {
        const self = try allocator.create(SecurityManager);
        self.* = .{
            .allocator = allocator,
            .gpg_manager = try gpg.GpgManager.init(allocator),
            .pkgbuild_analyzer = try analyzer.PkgbuildAnalyzer.init(allocator),
            .enforce_gpg = false, // Default to permissive for compatibility
            .min_trust_score = 4.0,
            .auto_import_keys = true,
        };
        return self;
    }
    
    pub fn deinit(self: *SecurityManager) void {
        self.gpg_manager.deinit();
        self.pkgbuild_analyzer.deinit();
        self.allocator.destroy(self);
    }
    
    pub fn analyzePackageSecurity(self: *SecurityManager, pkg: *Package, pkg_dir: []const u8) !SecurityAssessment {
        var assessment = SecurityAssessment{
            .package_name = try self.allocator.dupe(u8, pkg.name),
            .overall_score = pkg.trust_score,
            .gpg_verification = null,
            .pkgbuild_analysis = null,
            .risk_level = .safe,
            .recommendations = std.ArrayList([]const u8){},
            .is_safe_to_install = true,
        };
        
        // 1. GPG Signature Verification
        const gpg_result = self.gpg_manager.verifyPkgbuildSignature(pkg_dir) catch |err| switch (err) {
            error.OutOfMemory => return err,
            else => gpg.GpgVerification{
                .is_valid = false,
                .key_id = null,
                .fingerprint = null,
                .trust_level = .undefined,
                .signer_name = null,
                .signer_email = null,
                .creation_time = null,
                .expiration_time = null,
                .error_message = try self.allocator.dupe(u8, "GPG verification failed"),
            },
        };
        
        assessment.gpg_verification = gpg_result;
        
        // Update trust score based on GPG verification
        if (gpg_result.is_valid) {
            assessment.overall_score += gpg_result.getTrustScore();
            pkg.gpg_key = if (gpg_result.key_id) |key| try self.allocator.dupe(u8, key) else null;
        } else {
            if (self.enforce_gpg) {
                assessment.is_safe_to_install = false;
                try assessment.recommendations.append(self.allocator, try self.allocator.dupe(u8, "❌ GPG signature required but not found or invalid"));
            } else {
                try assessment.recommendations.append(self.allocator, try self.allocator.dupe(u8, "⚠️  No valid GPG signature found"));
            }
        }
        
        // 2. PKGBUILD Security Analysis
        const pkgbuild_path = try std.fs.path.join(self.allocator, &.{ pkg_dir, "PKGBUILD" });
        defer self.allocator.free(pkgbuild_path);
        
        const security_report = self.pkgbuild_analyzer.analyzeFile(pkgbuild_path) catch |err| switch (err) {
            error.FileNotFound => blk: {
                try assessment.recommendations.append(self.allocator, try self.allocator.dupe(u8, "❌ PKGBUILD file not found"));
                assessment.is_safe_to_install = false;
                break :blk analyzer.SecurityReport{
                    .violations = &.{},
                    .overall_score = -10.0,
                    .risk_level = .critical,
                    .summary = try self.allocator.dupe(u8, "PKGBUILD not found"),
                };
            },
            else => return err,
        };
        
        assessment.pkgbuild_analysis = security_report;
        assessment.overall_score += security_report.overall_score;
        
        // Add security-specific recommendations
        if (security_report.violations.len > 0) {
            for (security_report.violations) |violation| {
                const recommendation = try std.fmt.allocPrint(
                    self.allocator,
                    "🚨 {s}: {s} (line {d})",
                    .{ violation.pattern.name, violation.pattern.description, violation.line_number }
                );
                try assessment.recommendations.append(self.allocator, recommendation);
                
                if (violation.pattern.level == .critical) {
                    assessment.is_safe_to_install = false;
                }
            }
        }
        
        // 3. Determine overall risk level
        assessment.risk_level = self.determineOverallRisk(assessment.overall_score, security_report.risk_level, gpg_result.is_valid);
        
        // 4. Final safety assessment
        if (assessment.overall_score < self.min_trust_score) {
            assessment.is_safe_to_install = false;
            try assessment.recommendations.append(self.allocator, try std.fmt.allocPrint(
                self.allocator,
                "❌ Trust score {d:.1} below minimum threshold {d:.1}",
                .{ assessment.overall_score, self.min_trust_score }
            ));
        }
        
        // 5. Add positive recommendations for safe packages
        if (assessment.is_safe_to_install and security_report.violations.len == 0) {
            try assessment.recommendations.append(self.allocator, try self.allocator.dupe(u8, "✅ No security violations detected"));
            
            if (gpg_result.is_valid) {
                try assessment.recommendations.append(self.allocator, try self.allocator.dupe(u8, "✅ Valid GPG signature found"));
            }
            
            if (assessment.overall_score >= 8.0) {
                try assessment.recommendations.append(self.allocator, try self.allocator.dupe(u8, "⭐ High trust score - excellent package"));
            }
        }
        
        return assessment;
    }
    
    pub fn shouldBlockInstallation(self: *SecurityManager, assessment: *const SecurityAssessment) bool {
        _ = self;
        return !assessment.is_safe_to_install;
    }
    
    pub fn getSecurityPrompt(self: *SecurityManager, assessment: *const SecurityAssessment) ![]const u8 {
        var prompt = std.ArrayList(u8){};
        
        try prompt.appendSlice(self.allocator, "🔍 Security Assessment for ");
        try prompt.appendSlice(self.allocator, assessment.package_name);
        try prompt.appendSlice(self.allocator, ":\n\n");
        
        // Overall score
        try prompt.appendSlice(self.allocator, try std.fmt.allocPrint(
            self.allocator,
            "Trust Score: {d:.1}/10.0 ",
            .{assessment.overall_score}
        ));
        
        const score_badge = if (assessment.overall_score >= 8.0) "⭐ (Excellent)"
            else if (assessment.overall_score >= 6.0) "✅ (Good)"
            else if (assessment.overall_score >= 4.0) "⚠️  (Fair)"
            else "🚨 (Poor)";
        
        try prompt.appendSlice(self.allocator, score_badge);
        try prompt.appendSlice(self.allocator, "\n");
        
        // Risk level
        const risk_emoji = switch (assessment.risk_level) {
            .safe => "🟢",
            .warning => "🟡",
            .danger => "🟠",
            .critical => "🔴",
        };
        
        try prompt.appendSlice(self.allocator, try std.fmt.allocPrint(
            self.allocator,
            "Risk Level: {s} {s}\n\n",
            .{ risk_emoji, @tagName(assessment.risk_level) }
        ));
        
        // Security details
        if (assessment.gpg_verification) |gpg_ver| {
            if (gpg_ver.is_valid) {
                try prompt.appendSlice(self.allocator, "✅ GPG Signature: Valid");
                if (gpg_ver.signer_name) |signer| {
                    try prompt.appendSlice(self.allocator, " (");
                    try prompt.appendSlice(self.allocator, signer);
                    try prompt.appendSlice(self.allocator, ")");
                }
                try prompt.appendSlice(self.allocator, "\n");
            } else {
                try prompt.appendSlice(self.allocator, "❌ GPG Signature: ");
                try prompt.appendSlice(self.allocator, gpg_ver.error_message orelse "Invalid or missing");
                try prompt.appendSlice(self.allocator, "\n");
            }
        }
        
        if (assessment.pkgbuild_analysis) |analysis| {
            if (analysis.violations.len > 0) {
                try prompt.appendSlice(self.allocator, try std.fmt.allocPrint(
                    self.allocator,
                    "🚨 Security Issues: {d} found\n",
                    .{analysis.violations.len}
                ));
            } else {
                try prompt.appendSlice(self.allocator, "✅ Security Scan: No issues found\n");
            }
        }
        
        // Recommendations
        if (assessment.recommendations.items.len > 0) {
            try prompt.appendSlice(self.allocator, "\nRecommendations:\n");
            for (assessment.recommendations.items) |rec| {
                try prompt.appendSlice(self.allocator, "  ");
                try prompt.appendSlice(self.allocator, rec);
                try prompt.appendSlice(self.allocator, "\n");
            }
        }
        
        try prompt.appendSlice(self.allocator, "\n");
        
        if (assessment.is_safe_to_install) {
            try prompt.appendSlice(self.allocator, "Proceed with installation? [Y/n]: ");
        } else {
            try prompt.appendSlice(self.allocator, "⚠️  Installation not recommended. Continue anyway? [y/N]: ");
        }
        
        return prompt.toOwnedSlice(self.allocator);
    }
    
    fn determineOverallRisk(self: *SecurityManager, score: f32, pkgbuild_risk: analyzer.SecurityLevel, has_gpg: bool) analyzer.SecurityLevel {
        _ = self;
        
        // Critical conditions
        if (score < 2.0 or pkgbuild_risk == .critical) {
            return .critical;
        }
        
        // Danger conditions
        if (score < 4.0 or pkgbuild_risk == .danger or (!has_gpg and score < 6.0)) {
            return .danger;
        }
        
        // Warning conditions
        if (score < 6.0 or pkgbuild_risk == .warning or !has_gpg) {
            return .warning;
        }
        
        return .safe;
    }
};

pub const SecurityAssessment = struct {
    package_name: []const u8,
    overall_score: f32,
    gpg_verification: ?gpg.GpgVerification,
    pkgbuild_analysis: ?analyzer.SecurityReport,
    risk_level: analyzer.SecurityLevel,
    recommendations: std.ArrayList([]const u8),
    is_safe_to_install: bool,
    
    pub fn deinit(self: *SecurityAssessment, allocator: std.mem.Allocator) void {
        allocator.free(self.package_name);
        
        if (self.gpg_verification) |*gpg_ver| {
            if (gpg_ver.key_id) |key_id| allocator.free(key_id);
            if (gpg_ver.fingerprint) |fp| allocator.free(fp);
            if (gpg_ver.signer_name) |name| allocator.free(name);
            if (gpg_ver.signer_email) |email| allocator.free(email);
            if (gpg_ver.error_message) |msg| allocator.free(msg);
        }
        
        if (self.pkgbuild_analysis) |*analysis| {
            analysis.deinit(allocator);
        }
        
        for (self.recommendations.items) |rec| {
            allocator.free(rec);
        }
        self.recommendations.deinit(allocator);
    }
};