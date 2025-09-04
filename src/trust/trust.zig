const std = @import("std");
const zcrypto = @import("zcrypto");
const Package = @import("../core/package.zig").Package;
const tokioZ = @import("tokioZ");
const HttpClient = @import("../utils/http.zig").HttpClient;

pub const TrustLevel = enum {
    critical,  // 0-2
    low,       // 2-4
    medium,    // 4-6
    high,      // 6-8
    trusted,   // 8-10
    
    pub fn fromScore(score: f32) TrustLevel {
        if (score < 2.0) return .critical;
        if (score < 4.0) return .low;
        if (score < 6.0) return .medium;
        if (score < 8.0) return .high;
        return .trusted;
    }
    
    pub fn color(self: TrustLevel) []const u8 {
        return switch (self) {
            .critical => "\x1b[91m", // Bright red
            .low => "\x1b[93m",      // Bright yellow
            .medium => "\x1b[33m",   // Yellow
            .high => "\x1b[92m",     // Bright green
            .trusted => "\x1b[32m",  // Green
        };
    }
};

pub const SecurityViolation = enum {
    no_gpg_signature,
    invalid_signature,
    untrusted_maintainer,
    suspicious_source,
    known_malware,
    outdated_package,
    missing_checksum,
    checksum_mismatch,
    high_privilege_request,
};

pub const TrustEngine = struct {
    allocator: std.mem.Allocator,
    runtime: *tokioZ.Runtime,
    http_client: *HttpClient,
    crypto: *zcrypto.Context,
    
    // Trust database
    trusted_maintainers: std.StringHashMap(f32),
    blacklisted_packages: std.StringHashMap(void),
    malware_signatures: std.StringHashMap(void),
    
    // GPG keyring
    gpg_keyring: std.StringHashMap(GpgKey),
    
    pub fn init(allocator: std.mem.Allocator, runtime: *tokioZ.Runtime, http_client: *HttpClient) !*TrustEngine {
        var self = try allocator.create(TrustEngine);
        self.* = .{
            .allocator = allocator,
            .runtime = runtime,
            .http_client = http_client,
            .crypto = try zcrypto.Context.init(allocator),
            .trusted_maintainers = std.StringHashMap(f32).init(allocator),
            .blacklisted_packages = std.StringHashMap(void).init(allocator),
            .malware_signatures = std.StringHashMap(void).init(allocator),
            .gpg_keyring = std.StringHashMap(GpgKey).init(allocator),
        };
        
        // Load trust databases
        try self.loadTrustDatabase();
        try self.loadGpgKeyring();
        
        return self;
    }
    
    pub fn deinit(self: *TrustEngine) void {
        self.trusted_maintainers.deinit();
        self.blacklisted_packages.deinit();
        self.malware_signatures.deinit();
        
        var iter = self.gpg_keyring.iterator();
        while (iter.next()) |entry| {
            self.allocator.free(entry.key_ptr.*);
            entry.value_ptr.deinit(self.allocator);
        }
        self.gpg_keyring.deinit();
        
        self.crypto.deinit();
        self.allocator.destroy(self);
    }
    
    pub fn evaluatePackage(self: *TrustEngine, pkg: *Package) !TrustResult {
        var result = TrustResult{
            .trust_score = 5.0, // Base score
            .trust_level = .medium,
            .violations = std.ArrayList(SecurityViolation){},
            .warnings = std.ArrayList([]const u8){},
            .recommendations = std.ArrayList([]const u8){},
        };
        
        // Check if package is blacklisted
        if (self.blacklisted_packages.contains(pkg.name)) {
            result.trust_score = 0.0;
            try result.violations.append(self.allocator, .known_malware);
            try result.warnings.append(self.allocator, try self.allocator.dupe(u8, "Package is on malware blacklist"));
        }
        
        // Evaluate maintainer trustworthiness
        const maintainer_score = self.evaluateMaintainer(pkg.maintainer);
        result.trust_score += maintainer_score;
        
        if (maintainer_score < 1.0) {
            try result.violations.append(.untrusted_maintainer);
            try result.warnings.append(try std.allocator.dupe(u8, "Maintainer has low trust score"));
        }
        
        // Check package popularity and votes
        const popularity_score = self.calculatePopularityScore(pkg);
        result.trust_score += popularity_score;
        
        // Check for GPG signature
        if (pkg.gpg_key == null) {
            result.trust_score -= 1.0;
            try result.violations.append(.no_gpg_signature);
            try result.warnings.append(try self.allocator.dupe(u8, "Package has no GPG signature"));
        } else {
            const signature_valid = try self.verifyGpgSignature(pkg);
            if (!signature_valid) {
                result.trust_score -= 2.0;
                try result.violations.append(.invalid_signature);
                try result.warnings.append(try self.allocator.dupe(u8, "GPG signature verification failed"));
            } else {
                result.trust_score += 1.0;
            }
        }
        
        // Check package age and update frequency
        if (pkg.out_of_date) {
            result.trust_score -= 1.5;
            try result.violations.append(.outdated_package);
            try result.warnings.append(try self.allocator.dupe(u8, "Package is flagged as out of date"));
        }
        
        // Analyze source URLs for suspicious patterns
        for (pkg.source_urls) |source| {
            if (try self.isSuspiciousSource(source)) {
                result.trust_score -= 1.0;
                try result.violations.append(.suspicious_source);
                try result.warnings.append(try std.fmt.allocPrint(self.allocator, "Suspicious source: {s}", .{source}));
            }
        }
        
        // Check checksums
        if (pkg.checksum.len == 0) {
            result.trust_score -= 0.5;
            try result.violations.append(.missing_checksum);
            try result.recommendations.append(try self.allocator.dupe(u8, "Consider using packages with verified checksums"));
        }
        
        // Analyze build script for privilege escalation
        const has_privilege_escalation = try self.analyzePrivilegeEscalation(pkg);
        if (has_privilege_escalation) {
            result.trust_score -= 1.0;
            try result.violations.append(.high_privilege_request);
            try result.warnings.append(try self.allocator.dupe(u8, "Package requests elevated privileges"));
        }
        
        // Clamp score and determine level
        result.trust_score = @max(0.0, @min(10.0, result.trust_score));
        result.trust_level = TrustLevel.fromScore(result.trust_score);
        
        // Update package trust score
        pkg.trust_score = result.trust_score;
        
        return result;
    }
    
    pub fn verifyFileChecksum(self: *TrustEngine, file_path: []const u8, expected_checksum: []const u8) !bool {
        const file = try std.fs.openFileAbsolute(file_path, .{});
        defer file.close();
        
        // Read file in chunks and compute SHA256
        var hasher = try self.crypto.createHasher(.sha256);
        defer hasher.deinit();
        
        var buf: [8192]u8 = undefined;
        while (true) {
            const n = try file.read(&buf);
            if (n == 0) break;
            try hasher.update(buf[0..n]);
        }
        
        const computed_hash = try hasher.finalize();
        defer self.allocator.free(computed_hash);
        
        // Convert to hex string
        var hex_buf: [64]u8 = undefined;
        const hex_hash = try std.fmt.bufPrint(&hex_buf, "{}", .{std.fmt.fmtSliceHexLower(computed_hash)});
        
        return std.mem.eql(u8, hex_hash, expected_checksum);
    }
    
    pub fn signPackage(self: *TrustEngine, pkg_path: []const u8, private_key: []const u8) ![]const u8 {
        const file = try std.fs.openFileAbsolute(pkg_path, .{});
        defer file.close();
        
        const content = try file.readToEndAlloc(self.allocator, 100 * 1024 * 1024);
        defer self.allocator.free(content);
        
        // Create digital signature using zcrypto
        const signature = try self.crypto.sign(content, private_key);
        return signature;
    }
    
    pub fn verifyPackageSignature(self: *TrustEngine, pkg_path: []const u8, signature: []const u8, public_key: []const u8) !bool {
        const file = try std.fs.openFileAbsolute(pkg_path, .{});
        defer file.close();
        
        const content = try file.readToEndAlloc(self.allocator, 100 * 1024 * 1024);
        defer self.allocator.free(content);
        
        return self.crypto.verify(content, signature, public_key);
    }
    
    fn loadTrustDatabase(self: *TrustEngine) !void {
        // Load trusted maintainers database
        const trusted_maintainers = .{
            .{ "foutrelis", 9.5 },
            .{ "allan", 9.0 },
            .{ "anthraxx", 8.5 },
            .{ "heftig", 8.0 },
            .{ "barthalion", 7.5 },
        };
        
        inline for (trusted_maintainers) |maintainer| {
            try self.trusted_maintainers.put(
                try self.allocator.dupe(u8, maintainer.@"0"),
                maintainer.@"1"
            );
        }
        
        // Load known malware packages (would be loaded from external database)
        const malware_packages = .{
            "suspicious-package",
            "known-malware",
            "bitcoin-miner",
        };
        
        inline for (malware_packages) |pkg| {
            try self.blacklisted_packages.put(
                try self.allocator.dupe(u8, pkg),
                {}
            );
        }
    }
    
    fn loadGpgKeyring(self: *TrustEngine) !void {
        // In a real implementation, this would load from system keyring
        // For now, add some example keys
        
        const example_key = GpgKey{
            .fingerprint = try self.allocator.dupe(u8, "ABCD1234EFGH5678"),
            .user_id = try self.allocator.dupe(u8, "Example Maintainer <maintainer@example.com>"),
            .public_key = try self.allocator.dupe(u8, "example-public-key-data"),
            .creation_date = 1234567890,
            .expires = null,
            .trust_level = .ultimate,
        };
        
        try self.gpg_keyring.put(
            try self.allocator.dupe(u8, example_key.fingerprint),
            example_key
        );
    }
    
    fn evaluateMaintainer(self: *TrustEngine, maintainer: []const u8) f32 {
        if (self.trusted_maintainers.get(maintainer)) |score| {
            return score - 5.0; // Normalize to contribution score
        }
        return 0.0; // Unknown maintainer
    }
    
    fn calculatePopularityScore(self: *TrustEngine, pkg: *Package) f32 {
        _ = self;
        var score: f32 = 0.0;
        
        // Votes contribution (0-2 points)
        if (pkg.votes > 1000) {
            score += 2.0;
        } else if (pkg.votes > 500) {
            score += 1.5;
        } else if (pkg.votes > 100) {
            score += 1.0;
        } else if (pkg.votes > 10) {
            score += 0.5;
        }
        
        // Popularity contribution (0-1 point)
        if (pkg.popularity > 10.0) {
            score += 1.0;
        } else if (pkg.popularity > 1.0) {
            score += 0.5;
        }
        
        return score;
    }
    
    fn verifyGpgSignature(self: *TrustEngine, pkg: *Package) !bool {
        if (pkg.gpg_key == null) return false;
        
        // In a real implementation, this would verify the GPG signature
        // For now, just check if we have the key in our keyring
        return self.gpg_keyring.contains(pkg.gpg_key.?);
    }
    
    fn isSuspiciousSource(self: *TrustEngine, source_url: []const u8) !bool {
        _ = self;
        
        // Check for suspicious patterns in URLs
        const suspicious_patterns = .{
            "mega.nz",
            "bit.ly",
            "tinyurl.com",
            "pastebin.com",
            "raw.githubusercontent.com", // Sometimes used for malicious scripts
        };
        
        inline for (suspicious_patterns) |pattern| {
            if (std.mem.indexOf(u8, source_url, pattern) != null) {
                return true;
            }
        }
        
        // Check for non-HTTPS URLs
        if (!std.mem.startsWith(u8, source_url, "https://")) {
            return true;
        }
        
        return false;
    }
    
    fn analyzePrivilegeEscalation(self: *TrustEngine, pkg: *Package) !bool {
        _ = self;
        _ = pkg;
        
        // In a real implementation, this would analyze the PKGBUILD for:
        // - sudo usage
        // - chmod +s operations
        // - systemd service installations
        // - kernel module compilation
        // - etc.
        
        return false; // Placeholder
    }
};

pub const TrustResult = struct {
    trust_score: f32,
    trust_level: TrustLevel,
    violations: std.ArrayList(SecurityViolation),
    warnings: std.ArrayList([]const u8),
    recommendations: std.ArrayList([]const u8),
    
    pub fn deinit(self: *TrustResult, allocator: std.mem.Allocator) void {
        self.violations.deinit(allocator);
        for (self.warnings.items) |warning| {
            allocator.free(warning);
        }
        self.warnings.deinit(allocator);
        for (self.recommendations.items) |rec| {
            allocator.free(rec);
        }
        self.recommendations.deinit(allocator);
    }
    
    pub fn shouldAllowInstallation(self: TrustResult, min_score: f32, allow_untrusted: bool) bool {
        if (self.trust_score >= min_score) return true;
        return allow_untrusted;
    }
    
    pub fn printSummary(self: TrustResult, writer: anytype) !void {
        try writer.print("Trust Score: {s}{d:.1}/10{s} ({s})\n", .{
            self.trust_level.color(),
            self.trust_score,
            "\x1b[0m", // Reset color
            @tagName(self.trust_level),
        });
        
        if (self.violations.items.len > 0) {
            try writer.print("\nSecurity Violations:\n");
            for (self.violations.items) |violation| {
                try writer.print("  - {s}\n", .{@tagName(violation)});
            }
        }
        
        if (self.warnings.items.len > 0) {
            try writer.print("\nWarnings:\n");
            for (self.warnings.items) |warning| {
                try writer.print("  - {s}\n", .{warning});
            }
        }
        
        if (self.recommendations.items.len > 0) {
            try writer.print("\nRecommendations:\n");
            for (self.recommendations.items) |rec| {
                try writer.print("  - {s}\n", .{rec});
            }
        }
    }
};

pub const GpgKey = struct {
    fingerprint: []const u8,
    user_id: []const u8,
    public_key: []const u8,
    creation_date: u64,
    expires: ?u64,
    trust_level: GpgTrustLevel,
    
    pub fn deinit(self: *GpgKey, allocator: std.mem.Allocator) void {
        allocator.free(self.fingerprint);
        allocator.free(self.user_id);
        allocator.free(self.public_key);
    }
};

pub const GpgTrustLevel = enum {
    unknown,
    never,
    marginal,
    full,
    ultimate,
};