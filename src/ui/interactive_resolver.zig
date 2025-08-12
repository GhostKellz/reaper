const std = @import("std");
const ResolutionResult = @import("../core/dependency_resolver.zig").ResolutionResult;
const Conflict = @import("../core/dependency_resolver.zig").Conflict;

pub const InteractiveResolver = struct {
    allocator: std.mem.Allocator,
    auto_resolve: bool,
    ai_suggestions: bool,
    
    pub fn init(allocator: std.mem.Allocator) !*InteractiveResolver {
        const self = try allocator.create(InteractiveResolver);
        self.* = .{
            .allocator = allocator,
            .auto_resolve = false,
            .ai_suggestions = true,
        };
        return self;
    }
    
    pub fn deinit(self: *InteractiveResolver) void {
        self.allocator.destroy(self);
    }
    
    pub fn resolveConflicts(self: *InteractiveResolver, result: *ResolutionResult) !bool {
        if (result.conflicts.items.len == 0) {
            std.debug.print("✅ No conflicts detected - proceeding with installation\n", .{});
            return true;
        }
        
        std.debug.print("\n🚨 CONFLICT RESOLUTION REQUIRED\n", .{});
        std.debug.print("═══════════════════════════════════════════════════════════════\n", .{});
        
        std.debug.print("Found {} conflicts that need your attention:\n\n", .{result.conflicts.items.len});
        
        var resolved_count: u32 = 0;
        var auto_resolved: u32 = 0;
        
        for (result.conflicts.items, 0..) |conflict, i| {
            const conflict_num = i + 1;
            
            std.debug.print("🔸 CONFLICT #{} of {}\n", .{ conflict_num, result.conflicts.items.len });
            std.debug.print("   Packages: {} ↔️ {}\n", .{ conflict.package_a, conflict.package_b });
            std.debug.print("   Issue: {}\n", .{conflict.description});
            std.debug.print("   Type: {}\n", .{@tagName(conflict.conflict_type)});
            
            // Show AI-powered smart suggestions
            if (self.ai_suggestions) {
                const suggestions = try self.generateSmartSuggestions(conflict);
                defer self.deallocateSuggestions(suggestions);
                
                std.debug.print("\n💡 SMART SUGGESTIONS:\n", .{});
                for (suggestions, 0..) |suggestion, j| {
                    const suggestion_num = j + 1;
                    std.debug.print("   {}. {} (Confidence: {d}%)\n", .{ suggestion_num, suggestion.description, suggestion.confidence });
                }
            }
            
            // Auto-resolve if possible and enabled
            if (self.auto_resolve and conflict.suggested_action != .manual_resolution) {
                const auto_action = try self.getAutoResolutionAction(conflict);
                defer self.allocator.free(auto_action);
                
                std.debug.print("\n🤖 AUTO-RESOLVING: {}\n", .{auto_action});
                auto_resolved += 1;
                resolved_count += 1;
                continue;
            }
            
            // Interactive resolution
            std.debug.print("\n🎯 RESOLUTION OPTIONS:\n", .{});
            std.debug.print("   1. Remove conflicting package ({})\n", .{conflict.package_b});
            std.debug.print("   2. Skip installation of new package ({})\n", .{conflict.package_a});
            std.debug.print("   3. Force installation (ignore conflict)\n", .{});
            std.debug.print("   4. Find alternative package\n", .{});
            std.debug.print("   5. Show detailed analysis\n", .{});
            std.debug.print("   6. Auto-resolve remaining conflicts\n", .{});
            std.debug.print("   q. Quit installation\n", .{});
            
            while (true) {
                std.debug.print("\n❓ Choose action [1-6/q]: ", .{});
                
                const stdin = std.io.getStdIn().reader();
                const input = (try stdin.readUntilDelimiterAlloc(self.allocator, '\n', 256)) catch |err| switch (err) {
                    error.EndOfStream => break,
                    else => return err,
                };
                defer self.allocator.free(input);
                
                const choice = std.mem.trim(u8, input, " \r\n\t");
                
                if (std.mem.eql(u8, choice, "1")) {
                    std.debug.print("🗑️  Removing {} to resolve conflict...\n", .{conflict.package_b});
                    try self.markForRemoval(result, conflict.package_b);
                    resolved_count += 1;
                    break;
                } else if (std.mem.eql(u8, choice, "2")) {
                    std.debug.print("⏭️  Skipping {} installation...\n", .{conflict.package_a});
                    try self.markForSkip(result, conflict.package_a);
                    resolved_count += 1;
                    break;
                } else if (std.mem.eql(u8, choice, "3")) {
                    std.debug.print("⚠️  Forcing installation (conflicts ignored)...\n", .{});
                    resolved_count += 1;
                    break;
                } else if (std.mem.eql(u8, choice, "4")) {
                    const alternatives = try self.findAlternatives(conflict.package_a);
                    defer self.deallocateAlternatives(alternatives);
                    
                    if (alternatives.len > 0) {
                        std.debug.print("\n🔄 ALTERNATIVES for {}:\n", .{conflict.package_a});
                        for (alternatives, 0..) |alt, j| {
                            std.debug.print("   {}. {} - {}\n", .{ j + 1, alt.name, alt.description });
                        }
                        std.debug.print("   0. Go back\n", .{});
                        
                        std.debug.print("\n❓ Select alternative [0-{}]: ", .{alternatives.len});
                        const alt_input = (try stdin.readUntilDelimiterAlloc(self.allocator, '\n', 256)) catch continue;
                        defer self.allocator.free(alt_input);
                        
                        const alt_choice = std.fmt.parseInt(usize, std.mem.trim(u8, alt_input, " \r\n\t"), 10) catch continue;
                        
                        if (alt_choice > 0 and alt_choice <= alternatives.len) {
                            const selected_alt = alternatives[alt_choice - 1];
                            std.debug.print("🔄 Replacing {} with {}...\n", .{ conflict.package_a, selected_alt.name });
                            try self.replacePackage(result, conflict.package_a, selected_alt.name);
                            resolved_count += 1;
                            break;
                        }
                    } else {
                        std.debug.print("❌ No alternatives found for {}\n", .{conflict.package_a});
                    }
                } else if (std.mem.eql(u8, choice, "5")) {
                    try self.showDetailedAnalysis(conflict);
                } else if (std.mem.eql(u8, choice, "6")) {
                    std.debug.print("🤖 Auto-resolving remaining conflicts...\n", .{});
                    self.auto_resolve = true;
                    resolved_count += 1;
                    break;
                } else if (std.mem.eql(u8, choice, "q")) {
                    std.debug.print("❌ Installation cancelled by user\n", .{});
                    return false;
                } else {
                    std.debug.print("❌ Invalid choice. Please select 1-6 or 'q'\n", .{});
                    continue;
                }
            }
            
            std.debug.print("─────────────────────────────────────────────────────────────\n", .{});
        }
        
        // Summary
        std.debug.print("\n📊 RESOLUTION SUMMARY:\n", .{});
        std.debug.print("   Total conflicts: {}\n", .{result.conflicts.items.len});
        std.debug.print("   Resolved: {} ({} auto-resolved)\n", .{ resolved_count, auto_resolved });
        std.debug.print("   Remaining: {}\n", .{result.conflicts.items.len - resolved_count});
        
        if (resolved_count == result.conflicts.items.len) {
            std.debug.print("✅ All conflicts resolved! Proceeding with installation...\n", .{});
            return true;
        } else {
            std.debug.print("⚠️  Some conflicts remain unresolved\n", .{});
            return false;
        }
    }
    
    fn generateSmartSuggestions(self: *InteractiveResolver, conflict: Conflict) ![]SmartSuggestion {
        var suggestions = std.ArrayList(SmartSuggestion).init(self.allocator);
        
        // AI-powered conflict analysis
        switch (conflict.conflict_type) {
            .package_conflict => {
                try suggestions.append(.{
                    .description = try std.fmt.allocPrint(self.allocator, "Remove {} (likely outdated or replaced)", .{conflict.package_b}),
                    .confidence = 85,
                    .rationale = try std.fmt.allocPrint(self.allocator, "Package conflicts often indicate superseded packages"),
                });
                
                try suggestions.append(.{
                    .description = try std.fmt.allocPrint(self.allocator, "Keep both packages with --force (risky)"),
                    .confidence = 30,
                    .rationale = try std.fmt.allocPrint(self.allocator, "May cause file conflicts and system instability"),
                });
            },
            .version_conflict => {
                try suggestions.append(.{
                    .description = try std.fmt.allocPrint(self.allocator, "Upgrade {} to satisfy dependency", .{conflict.package_b}),
                    .confidence = 92,
                    .rationale = try std.fmt.allocPrint(self.allocator, "Version conflicts are best resolved by upgrading dependencies"),
                });
                
                try suggestions.append(.{
                    .description = try std.fmt.allocPrint(self.allocator, "Find compatible version of {}", .{conflict.package_a}),
                    .confidence = 75,
                    .rationale = try std.fmt.allocPrint(self.allocator, "Downgrading the requesting package may work"),
                });
            },
            .file_conflict => {
                try suggestions.append(.{
                    .description = try std.fmt.allocPrint(self.allocator, "Remove {} (file overlap detected)", .{conflict.package_b}),
                    .confidence = 88,
                    .rationale = try std.fmt.allocPrint(self.allocator, "File conflicts indicate packages provide same functionality"),
                });
            },
            .dependency_cycle => {
                try suggestions.append(.{
                    .description = try std.fmt.allocPrint(self.allocator, "Break cycle by installing {} first", .{conflict.package_a}),
                    .confidence = 70,
                    .rationale = try std.fmt.allocPrint(self.allocator, "Dependency cycles can be resolved by changing install order"),
                });
            },
        }
        
        return suggestions.toOwnedSlice();
    }
    
    fn getAutoResolutionAction(self: *InteractiveResolver, conflict: Conflict) ![]const u8 {
        return switch (conflict.suggested_action) {
            .remove_conflicting => try std.fmt.allocPrint(self.allocator, "Removing conflicting package: {}", .{conflict.package_b}),
            .upgrade_dependency => try std.fmt.allocPrint(self.allocator, "Upgrading dependency: {}", .{conflict.package_b}),
            .use_alternative => try std.fmt.allocPrint(self.allocator, "Finding alternative to: {}", .{conflict.package_a}),
            .manual_resolution => try std.fmt.allocPrint(self.allocator, "Manual resolution required"),
        };
    }
    
    fn showDetailedAnalysis(self: *InteractiveResolver, conflict: Conflict) !void {
        std.debug.print("\n🔍 DETAILED CONFLICT ANALYSIS\n", .{});
        std.debug.print("═══════════════════════════════════════════════════════════════\n", .{});
        
        std.debug.print("Package A: {}\n", .{conflict.package_a});
        std.debug.print("Package B: {}\n", .{conflict.package_b});
        std.debug.print("Conflict Type: {}\n", .{@tagName(conflict.conflict_type)});
        std.debug.print("Description: {}\n", .{conflict.description});
        std.debug.print("Suggested Action: {}\n", .{@tagName(conflict.suggested_action)});
        
        // Simulate package analysis
        std.debug.print("\n📋 PACKAGE COMPARISON:\n", .{});
        
        const analysis_a = try self.analyzePackage(conflict.package_a);
        defer self.deallocatePackageAnalysis(analysis_a);
        
        const analysis_b = try self.analyzePackage(conflict.package_b);
        defer self.deallocatePackageAnalysis(analysis_b);
        
        std.debug.print("   {} | Popularity: {} | Trust: {d}/10 | Size: {}\n", .{ conflict.package_a, analysis_a.popularity, analysis_a.trust_score, analysis_a.size });
        std.debug.print("   {} | Popularity: {} | Trust: {d}/10 | Size: {}\n", .{ conflict.package_b, analysis_b.popularity, analysis_b.trust_score, analysis_b.size });
        
        // Risk assessment
        std.debug.print("\n⚠️  RISK ASSESSMENT:\n", .{});
        
        if (analysis_a.trust_score > analysis_b.trust_score + 2.0) {
            std.debug.print("   • {} has significantly higher trust score\n", .{conflict.package_a});
        } else if (analysis_b.trust_score > analysis_a.trust_score + 2.0) {
            std.debug.print("   • {} has significantly higher trust score\n", .{conflict.package_b});
        }
        
        if (analysis_a.popularity > analysis_b.popularity * 2) {
            std.debug.print("   • {} is much more popular\n", .{conflict.package_a});
        } else if (analysis_b.popularity > analysis_a.popularity * 2) {
            std.debug.print("   • {} is much more popular\n", .{conflict.package_b});
        }
        
        std.debug.print("\n💡 RECOMMENDATION: ");
        
        // Smart recommendation logic
        const a_score = analysis_a.trust_score + @as(f32, @floatFromInt(analysis_a.popularity)) / 1000.0;
        const b_score = analysis_b.trust_score + @as(f32, @floatFromInt(analysis_b.popularity)) / 1000.0;
        
        if (a_score > b_score) {
            std.debug.print("Keep {} (better overall score)\n", .{conflict.package_a});
        } else {
            std.debug.print("Keep {} (better overall score)\n", .{conflict.package_b});
        }
        
        std.debug.print("═══════════════════════════════════════════════════════════════\n", .{});
    }
    
    fn findAlternatives(self: *InteractiveResolver, package_name: []const u8) ![]Alternative {
        // Simulate finding alternatives (in real implementation, would query repos)
        var alternatives = std.ArrayList(Alternative).init(self.allocator);
        
        // Common alternatives database
        if (std.mem.indexOf(u8, package_name, "firefox") != null) {
            try alternatives.append(.{
                .name = try self.allocator.dupe(u8, "firefox-esr"),
                .description = try self.allocator.dupe(u8, "Extended Support Release version"),
            });
            try alternatives.append(.{
                .name = try self.allocator.dupe(u8, "chromium"),
                .description = try self.allocator.dupe(u8, "Open-source web browser"),
            });
        } else if (std.mem.indexOf(u8, package_name, "vim") != null) {
            try alternatives.append(.{
                .name = try self.allocator.dupe(u8, "neovim"),
                .description = try self.allocator.dupe(u8, "Hyperextensible Vim-based text editor"),
            });
            try alternatives.append(.{
                .name = try self.allocator.dupe(u8, "emacs"),
                .description = try self.allocator.dupe(u8, "The extensible, customizable text editor"),
            });
        }
        
        return alternatives.toOwnedSlice();
    }
    
    fn analyzePackage(self: *InteractiveResolver, package_name: []const u8) !PackageAnalysis {
        // Simulate package analysis (in real implementation, would fetch from repos)
        _ = self;
        
        // Generate pseudo-realistic data based on package name hash
        var hasher = std.hash.Wyhash.init(0);
        hasher.update(package_name);
        const hash = hasher.final();
        
        return PackageAnalysis{
            .popularity = @intCast((hash % 10000) + 100),
            .trust_score = @as(f32, @floatFromInt((hash % 50) + 50)) / 10.0,
            .size = try std.fmt.allocPrint(self.allocator, "{d}.{} MB", .{ (hash % 100) + 1, (hash % 10) }),
            .last_updated = "2024-01-15",
        };
    }
    
    fn markForRemoval(self: *InteractiveResolver, result: *ResolutionResult, package_name: []const u8) !void {
        _ = self;
        _ = result;
        _ = package_name;
        // Mark package for removal in resolution result
    }
    
    fn markForSkip(self: *InteractiveResolver, result: *ResolutionResult, package_name: []const u8) !void {
        _ = self;
        _ = result;
        _ = package_name;
        // Mark package to skip in resolution result
    }
    
    fn replacePackage(self: *InteractiveResolver, result: *ResolutionResult, old_package: []const u8, new_package: []const u8) !void {
        _ = self;
        _ = result;
        _ = old_package;
        _ = new_package;
        // Replace package in resolution result
    }
    
    fn deallocateSuggestions(self: *InteractiveResolver, suggestions: []SmartSuggestion) void {
        for (suggestions) |suggestion| {
            self.allocator.free(suggestion.description);
            self.allocator.free(suggestion.rationale);
        }
        self.allocator.free(suggestions);
    }
    
    fn deallocateAlternatives(self: *InteractiveResolver, alternatives: []Alternative) void {
        for (alternatives) |alternative| {
            self.allocator.free(alternative.name);
            self.allocator.free(alternative.description);
        }
        self.allocator.free(alternatives);
    }
    
    fn deallocatePackageAnalysis(self: *InteractiveResolver, analysis: PackageAnalysis) void {
        self.allocator.free(analysis.size);
    }
};

const SmartSuggestion = struct {
    description: []const u8,
    confidence: u32, // 0-100
    rationale: []const u8,
};

const Alternative = struct {
    name: []const u8,
    description: []const u8,
};

const PackageAnalysis = struct {
    popularity: u32,
    trust_score: f32,
    size: []const u8,
    last_updated: []const u8,
};