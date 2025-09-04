const std = @import("std");

pub const TrustLevel = enum {
    undefined,
    never,
    marginal,
    full,
    ultimate,
    
    pub fn toFloat(self: TrustLevel) f32 {
        return switch (self) {
            .undefined => 0.0,
            .never => -2.0,
            .marginal => 0.5,
            .full => 1.5,
            .ultimate => 2.0,
        };
    }
};

pub const GpgVerification = struct {
    is_valid: bool,
    key_id: ?[]const u8,
    fingerprint: ?[]const u8,
    trust_level: TrustLevel,
    signer_name: ?[]const u8,
    signer_email: ?[]const u8,
    creation_time: ?u64,
    expiration_time: ?u64,
    error_message: ?[]const u8,
    
    pub fn getTrustScore(self: *const GpgVerification) f32 {
        if (!self.is_valid) return -1.0;
        return self.trust_level.toFloat();
    }
};

pub const KeyInfo = struct {
    key_id: []const u8,
    fingerprint: []const u8,
    user_ids: [][]const u8,
    trust_level: TrustLevel,
    creation_time: u64,
    expiration_time: ?u64,
    is_revoked: bool,
    is_expired: bool,
    
    pub fn deinit(self: *KeyInfo, allocator: std.mem.Allocator) void {
        allocator.free(self.key_id);
        allocator.free(self.fingerprint);
        for (self.user_ids) |uid| {
            allocator.free(uid);
        }
        allocator.free(self.user_ids);
    }
};

pub const GpgManager = struct {
    allocator: std.mem.Allocator,
    gpg_binary: []const u8,
    gnupg_home: ?[]const u8,
    keyservers: []const []const u8,
    auto_import_keys: bool,
    
    pub fn init(allocator: std.mem.Allocator) !*GpgManager {
        const self = try allocator.create(GpgManager);
        self.* = .{
            .allocator = allocator,
            .gpg_binary = "gpg",
            .gnupg_home = null,
            .keyservers = &.{
                "hkps://keys.openpgp.org",
                "hkps://keyserver.ubuntu.com",
                "hkps://pgp.mit.edu",
            },
            .auto_import_keys = true,
        };
        return self;
    }
    
    pub fn deinit(self: *GpgManager) void {
        if (self.gnupg_home) |home| {
            self.allocator.free(home);
        }
        self.allocator.destroy(self);
    }
    
    pub fn verifySignature(self: *GpgManager, file_path: []const u8, signature_path: []const u8) !GpgVerification {
        var args = std.ArrayList([]const u8){};
        defer args.deinit(self.allocator);
        
        try args.append(self.allocator, self.gpg_binary);
        try args.append(self.allocator, "--verify");
        try args.append(self.allocator, "--status-fd");
        try args.append(self.allocator, "1");
        
        if (self.gnupg_home) |home| {
            try args.append(self.allocator, "--homedir");
            try args.append(self.allocator, home);
        }
        
        try args.append(self.allocator, signature_path);
        try args.append(self.allocator, file_path);
        
        const result = try std.process.Child.run(.{
            .allocator = self.allocator,
            .argv = args.items,
        });
        defer self.allocator.free(result.stdout);
        defer self.allocator.free(result.stderr);
        
        return self.parseGpgOutput(result.stdout, result.stderr, result.term);
    }
    
    pub fn verifyPkgbuildSignature(self: *GpgManager, pkg_dir: []const u8) !GpgVerification {
        const pkgbuild_path = try std.fs.path.join(self.allocator, &.{ pkg_dir, "PKGBUILD" });
        defer self.allocator.free(pkgbuild_path);
        
        const sig_path = try std.fmt.allocPrint(self.allocator, "{s}.sig", .{pkgbuild_path});
        defer self.allocator.free(sig_path);
        
        // Check if signature file exists
        std.fs.accessAbsolute(sig_path, .{}) catch {
            return GpgVerification{
                .is_valid = false,
                .key_id = null,
                .fingerprint = null,
                .trust_level = .undefined,
                .signer_name = null,
                .signer_email = null,
                .creation_time = null,
                .expiration_time = null,
                .error_message = try self.allocator.dupe(u8, "No signature file found"),
            };
        };
        
        var verification = try self.verifySignature(pkgbuild_path, sig_path);
        
        // Auto-import missing keys if verification failed due to missing key
        if (!verification.is_valid and self.auto_import_keys) {
            if (verification.key_id) |key_id| {
                if (self.importKeyFromKeyserver(key_id)) {
                    // Retry verification after importing key
                    if (verification.error_message) |msg| {
                        self.allocator.free(msg);
                    }
                    verification = try self.verifySignature(pkgbuild_path, sig_path);
                } else |_| {
                    // Key import failed, keep original verification result
                }
            }
        }
        
        return verification;
    }
    
    pub fn importKeyFromKeyserver(self: *GpgManager, key_id: []const u8) !void {
        for (self.keyservers) |keyserver| {
            const import_result = self.tryImportFromKeyserver(key_id, keyserver) catch continue;
            if (import_result) return;
        }
        return error.KeyImportFailed;
    }
    
    fn tryImportFromKeyserver(self: *GpgManager, key_id: []const u8, keyserver: []const u8) !bool {
        var args = std.ArrayList([]const u8){};
        defer args.deinit(self.allocator);
        
        try args.append(self.allocator, self.gpg_binary);
        try args.append(self.allocator, "--keyserver");
        try args.append(self.allocator, keyserver);
        try args.append(self.allocator, "--recv-keys");
        
        if (self.gnupg_home) |home| {
            try args.append(self.allocator, "--homedir");
            try args.append(self.allocator, home);
        }
        
        try args.append(self.allocator, key_id);
        
        const result = try std.process.Child.run(.{
            .allocator = self.allocator,
            .argv = args.items,
        });
        defer self.allocator.free(result.stdout);
        defer self.allocator.free(result.stderr);
        
        return result.term == .Exited and result.term.Exited == 0;
    }
    
    pub fn getKeyInfo(self: *GpgManager, key_id: []const u8) !?KeyInfo {
        var args = std.ArrayList([]const u8){};
        defer args.deinit(self.allocator);
        
        try args.append(self.allocator, self.gpg_binary);
        try args.append(self.allocator, "--list-keys");
        try args.append(self.allocator, "--with-colons");
        try args.append(self.allocator, "--fixed-list-mode");
        
        if (self.gnupg_home) |home| {
            try args.append(self.allocator, "--homedir");
            try args.append(self.allocator, home);
        }
        
        try args.append(self.allocator, key_id);
        
        const result = try std.process.Child.run(.{
            .allocator = self.allocator,
            .argv = args.items,
        });
        defer self.allocator.free(result.stdout);
        defer self.allocator.free(result.stderr);
        
        if (result.term != .Exited or result.term.Exited != 0) {
            return null;
        }
        
        return self.parseKeyInfo(result.stdout);
    }
    
    pub fn getTrustLevel(self: *GpgManager, key_id: []const u8) !TrustLevel {
        if (try self.getKeyInfo(key_id)) |key_info| {
            defer key_info.deinit(self.allocator);
            return key_info.trust_level;
        }
        return .undefined;
    }
    
    pub fn autoFetchMissingKeys(self: *GpgManager, signature_error: []const u8) !void {
        // Parse error message to extract key ID
        if (std.mem.indexOf(u8, signature_error, "NO_PUBKEY")) |start| {
            const key_start = start + 10; // Length of "NO_PUBKEY "
            if (key_start < signature_error.len) {
                var key_end = key_start;
                while (key_end < signature_error.len and std.ascii.isAlphanumeric(signature_error[key_end])) {
                    key_end += 1;
                }
                
                if (key_end > key_start) {
                    const key_id = signature_error[key_start..key_end];
                    try self.importKeyFromKeyserver(key_id);
                }
            }
        }
    }
    
    fn parseGpgOutput(self: *GpgManager, stdout: []const u8, stderr: []const u8, term: std.process.Child.Term) !GpgVerification {
        var verification = GpgVerification{
            .is_valid = false,
            .key_id = null,
            .fingerprint = null,
            .trust_level = .undefined,
            .signer_name = null,
            .signer_email = null,
            .creation_time = null,
            .expiration_time = null,
            .error_message = null,
        };
        
        if (term != .Exited or term.Exited != 0) {
            verification.error_message = try self.allocator.dupe(u8, stderr);
            return verification;
        }
        
        // Parse GPG status output
        var lines = std.mem.tokenizeScalar(u8, stdout, '\n');
        while (lines.next()) |line| {
            if (std.mem.startsWith(u8, line, "[GNUPG:] GOODSIG")) {
                verification.is_valid = true;
                // Extract key ID and signer info
                var parts = std.mem.tokenizeScalar(u8, line, ' ');
                _ = parts.next(); // Skip "[GNUPG:]"
                _ = parts.next(); // Skip "GOODSIG"
                
                if (parts.next()) |key_id| {
                    verification.key_id = try self.allocator.dupe(u8, key_id);
                }
                
                // Rest is signer name/email
                var signer_parts = std.ArrayList(u8){};
                defer signer_parts.deinit(self.allocator);
                
                while (parts.next()) |part| {
                    if (signer_parts.items.len > 0) {
                        try signer_parts.append(self.allocator, ' ');
                    }
                    try signer_parts.appendSlice(self.allocator, part);
                }
                
                if (signer_parts.items.len > 0) {
                    verification.signer_name = try signer_parts.toOwnedSlice(self.allocator);
                }
            } else if (std.mem.startsWith(u8, line, "[GNUPG:] TRUST_")) {
                // Extract trust level
                if (std.mem.indexOf(u8, line, "TRUST_ULTIMATE")) |_| {
                    verification.trust_level = .ultimate;
                } else if (std.mem.indexOf(u8, line, "TRUST_FULL")) |_| {
                    verification.trust_level = .full;
                } else if (std.mem.indexOf(u8, line, "TRUST_MARGINAL")) |_| {
                    verification.trust_level = .marginal;
                } else if (std.mem.indexOf(u8, line, "TRUST_NEVER")) |_| {
                    verification.trust_level = .never;
                }
            } else if (std.mem.startsWith(u8, line, "[GNUPG:] BADSIG")) {
                verification.is_valid = false;
                verification.error_message = try self.allocator.dupe(u8, "Bad signature");
            } else if (std.mem.startsWith(u8, line, "[GNUPG:] NO_PUBKEY")) {
                verification.is_valid = false;
                verification.error_message = try self.allocator.dupe(u8, "Public key not available");
                
                // Extract missing key ID
                var parts = std.mem.tokenizeScalar(u8, line, ' ');
                _ = parts.next(); // Skip "[GNUPG:]"
                _ = parts.next(); // Skip "NO_PUBKEY"
                
                if (parts.next()) |key_id| {
                    verification.key_id = try self.allocator.dupe(u8, key_id);
                }
            }
        }
        
        return verification;
    }
    
    fn parseKeyInfo(self: *GpgManager, output: []const u8) !?KeyInfo {
        var lines = std.mem.tokenizeScalar(u8, output, '\n');
        var key_info: ?KeyInfo = null;
        var user_ids = std.ArrayList([]const u8){};
        
        while (lines.next()) |line| {
            var fields = std.mem.tokenizeScalar(u8, line, ':');
            const record_type = fields.next() orelse continue;
            
            if (std.mem.eql(u8, record_type, "pub")) {
                // Public key record
                _ = fields.next(); // Skip validity
                _ = fields.next(); // Skip key length
                _ = fields.next(); // Skip algorithm
                
                const key_id = fields.next() orelse continue;
                const creation_date = fields.next() orelse "0";
                const expiration_date = fields.next();
                
                key_info = KeyInfo{
                    .key_id = try self.allocator.dupe(u8, key_id),
                    .fingerprint = try self.allocator.dupe(u8, key_id), // Simplified
                    .user_ids = &.{},
                    .trust_level = .undefined,
                    .creation_time = std.fmt.parseInt(u64, creation_date, 10) catch 0,
                    .expiration_time = if (expiration_date) |exp| std.fmt.parseInt(u64, exp, 10) catch null else null,
                    .is_revoked = false,
                    .is_expired = false,
                };
            } else if (std.mem.eql(u8, record_type, "uid") and key_info != null) {
                // User ID record
                _ = fields.next(); // Skip validity
                _ = fields.next(); // Skip creation date
                _ = fields.next(); // Skip expiration date
                _ = fields.next(); // Skip hash
                _ = fields.next(); // Skip class
                
                const user_id = fields.next() orelse continue;
                try user_ids.append(self.allocator, try self.allocator.dupe(u8, user_id));
            }
        }
        
        if (key_info) |*info| {
            info.user_ids = try user_ids.toOwnedSlice();
            return info.*;
        }
        
        // Clean up if no key found
        for (user_ids.items) |uid| {
            self.allocator.free(uid);
        }
        user_ids.deinit(self.allocator);
        
        return null;
    }
};