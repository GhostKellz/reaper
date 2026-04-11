# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.8.1] - 2026-04-11

### 🐛 Fixed
- **Install script binary path detection** - now correctly finds binaries in both `target/release/` and `target/x86_64-unknown-linux-gnu/release/`
- **Legacy naming cleanup** - renamed `brew.lua` to `reap.lua` (leftover from ghostbrew)
- **Install script config creation** - now creates `pinned.toml` and `reap.lua` so `reap doctor` passes on fresh installs

### 🔧 Changed
- **Dynamic versioning** - CLI version now uses `env!("CARGO_PKG_VERSION")` instead of hardcoded string

## [0.8.0] - 2026-04-06

### 🔄 Added - Transaction Rollback System
- **Transaction journal** recording all install/upgrade/remove operations
- **Rollback commands**: `reap rollback list`, `show`, `dry-run`, `apply`
- **Artifact-backed rollback** using pacman cache and retained AUR builds
- **Rollback preview** showing planned downgrades, reinstalls, and removals
- **Dependency analysis** detecting potential breaks before rollback execution
- **Rollback attempt tracking** with success/partial/failed status recording
- **Split package tracking** - AUR split packages (e.g., `-debug`) now tracked in transactions
- **Post-rollback verification** confirming package states match expectations
- **Non-zero exit codes** for scripting reliability on rollback failures

### 🔧 Added - Enhanced Configuration System
- **Multi-source config merging** with precedence hierarchy
- **Environment variable overrides** for all configuration options
- **Profile-aware configuration** inheritance
- **Robust validation** with detailed error reporting

### 📦 Added - Development Package Tracking
- **VCS package monitoring** for Git, SVN, Mercurial, Bazaar
- **Automated update detection** for `-git` and other development packages
- **Development package database** with JSON persistence
- **Smart caching** and build directory management

### 🔗 Added - Advanced Dependency Resolution
- **Topological dependency sorting** for correct install order
- **Circular dependency detection** and resolution strategies
- **Conflict analysis** with detailed reporting
- **Dependency graph visualization**
- **Install plan generation** with reasoning

### 🛠️ Added - Infrastructure
- Error handling improvements with multi-error aggregation
- News integration for Arch Linux news parsing and notifications
- Interactive search & install with dynamic filtering
- Advanced build system with comprehensive makepkg integration
- Repository management with multi-repo support and priorities
- GitHub Actions CI/CD workflows for build, test, and release
- Docker support with multi-platform builds
- Makefile for local development workflow

### 🔧 Changed
- **Trust CLI** commands now: `score`, `scan`, `stats`, `update`
- **Transaction recording** captures all affected packages including dependencies
- **AUR installs** now detect and track split packages automatically

### 🐛 Fixed
- **Version display** in `rollback show` now shows actual version instead of `:`
- **Split package cleanup** - rollback now removes all packages from a transaction
- **Cache test flakiness** - tests now use unique keys to avoid parallel test interference
- **Documentation drift** - `docs/usage/commands.md` reconciled with actual CLI surface
- Various compilation warnings and code improvements

## [0.5.0] - 2025-06-16

### 🛡️ Added - Trust & Security Engine
- **Real-time trust scoring system** with 0-10 scale for all packages
- **Security badges**: 🛡️ TRUSTED, ✅ VERIFIED, ⚠️ CAUTION, 🚨 RISKY, ❌ UNSAFE
- **PKGBUILD security analysis** detecting suspicious patterns and operations
- **PGP signature verification** with comprehensive key validation
- **Publisher verification** system for package maintainer authentication
- **Security flag detection** for network access, system permissions, file operations
- **Trust database caching** for improved performance and offline analysis

### ⭐ Added - Community Rating System
- **AUR integration** showing real community votes and popularity scores
- **User rating system** with 1-5 star ratings and optional comments
- **Visual star display** (⭐⭐⭐⭐⭐) in TUI and CLI output
- **Community reviews** with helpful vote tracking
- **Rating persistence** with local storage and synchronization
- **Combined trust + rating display** in package listings

### 👤 Added - Multi-Profile Management
- **Profile system** with switchable configurations for different workflows
- **Profile templates**: Developer, Gaming, Minimal presets with optimized settings
- **Profile-aware operations** adapting behavior to active profile
- **Custom profile creation** with granular setting control
- **Backend prioritization** per profile (tap → aur → flatpak, etc.)
- **Security policy inheritance** from profiles (strict/moderate/permissive)
- **Performance tuning** with profile-specific parallel job counts

### 🔧 Added - Enhanced AUR Operations
- **Manual PKGBUILD fetching** with comprehensive parsing and analysis
- **Interactive PKGBUILD editing** with safety confirmations and validation
- **Advanced dependency resolution** with circular dependency detection
- **Conflict detection system** for package, file, and version conflicts
- **PKGBUILD diff viewer** with colored output showing changes
- **Dependency tree analysis** with conflict prediction before installation
- **Build artifact caching** for improved performance

### 📋 Added - Enhanced Interactive TUI
- **Five-tab interface**: Search, Queue, Log, Profiles, System monitoring
- **Live build progress** with real-time makepkg output streaming
- **Trust score integration** in all package listings and search results
- **Rating display** with star ratings throughout the interface
- **Package details panel** with comprehensive information (TAB to toggle)
- **Interactive hotkeys**: `t` trust details, `r` rate package, `d` diff, `p` profiles
- **System statistics dashboard** with package distribution charts
- **Real-time log filtering** with colored output and scrolling

### 💬 Added - Interactive Prompts & Safety
- **Smart confirmation prompts** for dangerous operations
- **PKGBUILD editing warnings** with security implications
- **Package removal confirmations** showing affected packages
- **Interactive package selection** with numbered menus
- **Diff-based install confirmation** showing changes before proceeding
- **Security override prompts** for risky packages with explicit warnings

### 🚀 Added - Intelligent Systems
- **Advanced dependency resolver** with conflict prediction and resolution
- **Real-time analytics** tracking installation performance and success rates
- **Build progress estimation** with ETA calculations
- **System health monitoring** with package status tracking
- **Profile-based security policies** enforcing appropriate security levels
- **Trust-guided decision making** throughout package operations

### 🔧 Added - CLI Enhancements
- `reap trust score <pkg>` - Analyze package security and trust
- `reap trust scan` - System-wide security audit of installed packages
- `reap trust stats` - Display trust statistics and security overview
- `reap rate <pkg> <stars> [comment]` - Rate packages with stars and comments
- `reap profile create/switch/list/show/delete` - Complete profile management
- `reap aur fetch/edit/deps` - Advanced AUR operations and analysis
- `reap install --diff` - Show PKGBUILD diff before installation
- Enhanced search results with trust badges and ratings

### 🔄 Changed
- **Search results** now include trust badges and community ratings
- **Installation process** now includes trust analysis and profile-aware settings
- **TUI interface** completely redesigned with tabbed layout and live monitoring
- **Package operations** now respect active profile security and performance settings
- **Error handling** improved with better user feedback and recovery options

### 🔧 Technical Improvements
- **Async architecture** with improved concurrent operations
- **Caching system** for trust scores, ratings, and PKGBUILD data
- **Performance optimization** with profile-based parallel job management
- **Memory efficiency** with streaming operations for large outputs
- **Security-first design** with comprehensive input validation

### 📚 Documentation
- Added comprehensive FEATURES.md with detailed feature documentation
- Added SECURITY.md with security best practices and trust system guide
- Added API.md with complete developer API reference
- Added ARCHITECTURE.md documenting system design and module structure
- Updated README.md with v0.5.0 feature highlights and examples
- Enhanced CONTRIBUTING.md with security-focused development guidelines

## [0.4.0] - 2024-12-XX

### Added
- Refactored `resolve_and_install_deps` to use dynamic package lists and proper async return types
- Fully implemented recursive AUR + repo dependency resolution with deduplication
- `pkgb` now parsed and printed via `parse_pkgname_ver` to eliminate unused variable warnings
- Fixed Clippy-critical errors (E0308, E0271) blocking build; reduced total warnings significantly
- Updated core.rs to use clean `Box::pin(async move { ... })` with correct `Result<(), ()>` wrapping

### Fixed
- Critical compilation errors preventing build
- Async function return type mismatches
- Unused variable warnings throughout codebase
- Dependency resolution edge cases

## [0.3.0] - 2024-11-XX

### Added
- Interactive TUI for package management
- Multi-backend support (AUR, Flatpak, Pacman)
- Tap system for custom repositories
- GPG verification support
- Parallel operations

### Fixed
- Package detection accuracy
- Installation reliability
- Error handling improvements
