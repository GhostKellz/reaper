# 📋 reap Commands Reference

Complete reference for all reap commands, options, and usage patterns.

## Table of Contents

1. [General Usage](#general-usage)
2. [PKGBUILD Commands](#pkgbuild-commands)
3. [reap.toml Commands](#reaptoml-commands)
4. [Native Compilation](#native-compilation)
5. [AUR Integration](#aur-integration)
6. [Multi-Architecture Builds](#multi-architecture-builds)
7. [Configuration & Cache](#configuration--cache)
8. [Examples & Workflows](#examples--workflows)

---

## General Usage

### Basic Syntax
```bash
reap <COMMAND> [OPTIONS] [ARGUMENTS]
```

### Global Options
```bash
--help, -h          Show help message
--version, -v       Show version information
--verbose           Enable verbose output
--quiet             Suppress non-essential output
--no-cache          Disable build caching
--cache-size=SIZE   Set cache size limit (MB)
```

### Environment Variables
```bash
REAP_CACHE_DIR     Custom cache directory (default: ~/.cache/reap)
REAP_CACHE_SIZE    Cache size limit in MB (default: 100)
REAP_DEBUG         Enable debug output (0/1)
REAP_PARALLEL      Max parallel jobs (default: 4)
```

---

## PKGBUILD Commands

Traditional PKGBUILD-based package building compatible with makepkg.

### `reap init`
Initialize a new build workspace with example PKGBUILD.

```bash
reap init [DIRECTORY]
```

**Options:**
- `DIRECTORY` - Target directory (default: current directory)

**Example:**
```bash
reap init
reap init /tmp/my-package
```

**Output:**
```
🚀 Initializing reap workspace...
✅ Created example PKGBUILD
📝 Edit the PKGBUILD file and run 'reap build' to get started!
```

### `reap build`
Build a package from PKGBUILD.

```bash
reap build [PKGBUILD_PATH] [OPTIONS]
```

**Options:**
- `PKGBUILD_PATH` - Path to PKGBUILD file (default: ./PKGBUILD)
- `--force` - Force rebuild even if cached
- `--no-deps` - Skip dependency checking
- `--no-cache` - Disable caching for this build

**Examples:**
```bash
reap build                           # Build ./PKGBUILD
reap build /path/to/PKGBUILD        # Build specific PKGBUILD
reap build --force                  # Force rebuild
reap build --no-deps               # Skip dependency check
```

**Process:**
1. Parse PKGBUILD file
2. Validate required fields
3. Check dependencies with pacman
4. Download sources in parallel
5. Verify SHA256 checksums
6. Execute prepare() function
7. Execute build() function
8. Execute check() function (optional)
9. Cache successful build

### `reap package`
Build and create .pkg.tar.zst package archive.

```bash
reap package [PKGBUILD_PATH] [OPTIONS]
```

**Options:**
- `--sign=KEY` - GPG sign package with specified key
- `--output=DIR` - Output directory for package
- `--compression=LEVEL` - zstd compression level (1-22, default: 3)

**Examples:**
```bash
reap package                        # Build and package
reap package --sign=mykey@email.com # Sign with GPG
reap package --output=/tmp/packages # Custom output directory
```

**Output:**
```
📦 Building from: ./PKGBUILD
✅ Parsed PKGBUILD: hello-world v1.0.0-1
==> Building package hello-world-1.0.0-1...
✅ Build completed successfully
==> Packaging files...
✅ Package created: hello-world-1.0.0-1-x86_64.pkg.tar.zst (156 KB)
```

### `reap clean`
Clean build artifacts and cache.

```bash
reap clean [OPTIONS]
```

**Options:**
- `--all` - Clean everything including cache
- `--cache-only` - Clean only cache, keep build dirs
- `--builds-only` - Clean only build dirs, keep cache

**Examples:**
```bash
reap clean                 # Clean build directories
reap clean --all          # Clean everything
reap clean --cache-only   # Clean only cache
```

---

## reap.toml Commands

Modern declarative configuration format as an alternative to PKGBUILD.

### `reap toml-init`
Create a new reap.toml configuration file.

```bash
reap toml-init [DIRECTORY]
```

**Examples:**
```bash
reap toml-init              # Create reap.toml in current directory
reap toml-init /tmp/project # Create in specific directory
```

**Generated reap.toml:**
```toml
[package]
name = "my-project"
version = "1.0.0"
description = "A project built with reap"
url = "https://github.com/username/my-project"
license = ["MIT"]
arch = ["x86_64"]
maintainer = "Your Name <your.email@example.com>"

[build]
type = "auto"  # auto-detect: zig, c, cpp, make, cmake, meson, custom
sources = [
    "https://github.com/username/my-project/archive/v${version}.tar.gz"
]
checksums = ["SKIP"]

[dependencies]
runtime = ["glibc"]
build = ["gcc", "make"]
# aur = ["some-aur-package"]

[[targets]]
name = "linux-x64"
triple = "x86_64-linux-gnu"
optimize = "ReleaseFast"
```

### `reap toml-build`
Build from reap.toml configuration.

```bash
reap toml-build [TOML_FILE] [OPTIONS]
```

**Options:**
- `TOML_FILE` - Path to reap.toml file (default: ./reap.toml)
- `--target=TARGET` - Build specific target only
- `--no-aur` - Skip AUR dependency resolution

**Examples:**
```bash
reap toml-build                    # Build from ./reap.toml
reap toml-build config/build.toml  # Use specific file
reap toml-build --target=linux-x64 # Build only one target
reap toml-build --no-aur          # Skip AUR dependencies
```

**Process:**
1. Parse reap.toml configuration
2. Validate configuration structure
3. Resolve AUR dependencies automatically
4. Generate temporary PKGBUILD
5. Execute standard build pipeline
6. Build for all specified targets

**Advantages over PKGBUILD:**
- Type-safe configuration
- Automatic AUR dependency resolution
- Multi-target builds built-in
- No bash scripting required for simple projects

---

## Native Compilation

Direct compilation of Zig, C, and C++ projects without PKGBUILD.

### `reap detect`
Auto-detect project type and display information.

```bash
reap detect [PROJECT_DIR]
```

**Examples:**
```bash
reap detect                # Detect current directory
reap detect /path/to/proj  # Detect specific directory
```

**Output for Zig project:**
```
🔍 Detecting project type...
✅ Detected: Zig Project
   Name: hello-zig
   Version: 1.0.0
   Build file: /path/to/build.zig
   Source root: /path/to/src
   Targets: native
   Dependencies: (none)
```

**Output for C project:**
```
🔍 Detecting project type...
✅ Detected: C Project  
   Name: hello-c
   Sources: main.c, utils.c
   Headers: utils.h
   Suggested flags: -O2 -Wall -Wextra
```

### `reap compile`
Compile native project using Zig compiler.

```bash
reap compile [PROJECT_DIR] [OPTIONS]
```

**Options:**
- `--release` - Build in release mode (optimized)
- `--debug` - Build in debug mode (default)
- `--target=TARGET` - Specify target triple
- `--output=PATH` - Output binary path

**Examples:**
```bash
reap compile                           # Debug build
reap compile --release                 # Release build
reap compile --target=x86_64-windows-gnu # Cross-compile
reap compile --output=bin/myapp       # Custom output path
```

**Supported Projects:**
- **Zig**: Uses `zig build` with proper flags
- **C/C++**: Uses `zig cc` for compilation
- **Mixed**: Zig projects with C dependencies

### `reap cross`
Cross-compile for different target architectures.

```bash
reap cross <TARGET> [PROJECT_DIR] [OPTIONS]
```

**Required:**
- `TARGET` - Target triple (e.g., x86_64-windows-gnu)

**Options:**
- `--release` - Build in release mode
- `--features=LIST` - Comma-separated CPU features

**Supported Targets:**
```bash
# Desktop platforms
x86_64-linux-gnu        # Linux x64 (glibc)
x86_64-linux-musl       # Linux x64 (musl)
x86_64-windows-gnu      # Windows x64
x86_64-macos            # macOS x64

# ARM platforms  
aarch64-linux-gnu       # ARM64 Linux (glibc)
aarch64-linux-musl      # ARM64 Linux (musl)
aarch64-macos           # macOS ARM64 (M1/M2)
arm-linux-gnueabihf     # ARM32 Linux

# Other architectures
riscv64-linux-gnu       # RISC-V 64-bit
wasm32-wasi             # WebAssembly (WASI)
wasm32-freestanding     # WebAssembly (bare)
```

**Examples:**
```bash
reap cross x86_64-windows-gnu          # Windows executable
reap cross aarch64-linux-gnu --release # ARM64 Linux (optimized)
reap cross wasm32-wasi                 # WebAssembly
reap cross riscv64-linux-gnu          # RISC-V
```

**Output:**
```
🎯 Cross-compiling for: x86_64-windows-gnu
✅ Detected: Zig Project (hello-zig v1.0.0)
==> Building with target: x86_64-windows-gnu
==> Creating package structure...
✅ Cross-compiled: hello-zig.exe
🎉 Native compilation completed (release mode)!
```

---

## AUR Integration

Automatic AUR package search, dependency resolution, and building.

### `reap -Ss`
Search for packages in AUR and official repositories.

```bash
reap -Ss <PACKAGE_NAME>
```

**Examples:**
```bash
reap -Ss yay              # Search for yay
reap -Ss "neovim-nightly" # Search with quotes
```

**Output:**
```
🔍 Searching for: yay
📦 Found: yay v12.1.3 (AUR)
   Description: Yet another yogurt. Pacman wrapper and AUR helper written in go.
   URL: https://github.com/Jguer/yay
   Clone URL: https://aur.archlinux.org/yay.git
💡 Run 'reap -S yay' to install
```

### `reap -S`
Install packages from AUR or official repositories with automatic dependency resolution.

```bash
reap -S <PACKAGE_NAME> [OPTIONS]
```

**Options:**
- `--nodeps` - Skip dependency resolution
- `--force` - Force reinstall if already installed
- `--dry-run` - Show what would be installed without doing it

**Examples:**
```bash
reap -S yay                # Install yay and dependencies
reap -S paru --nodeps      # Install without dependencies
reap -S discord --dry-run  # Show install plan
```

**Process:**
1. Search AUR and official repos for package metadata
2. Recursively resolve all dependencies
3. Create dependency build order
4. Clone git repositories for AUR packages
5. Build and install in correct order

**Output:**
```
📦 Installing package: yay
==> Searching for: yay
✅ Found in AUR: yay v12.1.3
==> Resolving dependencies...
   📦 go (official)
   📦 git (official)
   📦 yay (AUR)
==> Build order: 3 packages
==> Cloning AUR package: yay
✅ Cloned to: ~/.cache/reap/aur/yay
==> Building AUR package: yay
✅ Successfully built and installed: yay
🎉 All dependencies installed successfully!
```

---

## Multi-Architecture Builds

Build projects for multiple target architectures simultaneously.

### `reap multi-build`
Build for multiple architectures in parallel.

```bash
reap multi-build <TARGET_SET> [PROJECT_DIR] [OPTIONS]
```

**Target Sets:**
- `desktop` - Linux x64, Windows x64, macOS x64
- `embedded` - ARM64, ARM32, RISC-V Linux
- `web` - WebAssembly (WASI, freestanding)
- `all` - All supported platforms

**Options:**
- `--parallel=N` - Max parallel builds (default: 4)
- `--release` - Build all targets in release mode
- `--output=DIR` - Output directory (default: multi-arch-builds)

**Examples:**
```bash
reap multi-build desktop                  # Build for desktop platforms
reap multi-build embedded --release      # Optimized embedded builds
reap multi-build all --parallel=8        # All platforms, 8 parallel jobs
reap multi-build web --output=/tmp/wasm  # WebAssembly builds
```

**Output:**
```
🏗️  Starting multi-architecture build: desktop
    Max parallel builds: 4
==> Starting build for linux-x64 (x86_64-linux-gnu)
==> Starting build for windows-x64 (x86_64-windows-gnu)  
==> Starting build for macos-x64 (x86_64-macos)
✅ linux-x64: Built in 1234ms, size: 2048KB
✅ windows-x64: Built in 1456ms, size: 2156KB
✅ macos-x64: Built in 1321ms, size: 2089KB

==> Multi-Architecture Build Report
====================================
✅ linux-x64        | x86_64-linux-gnu      |   1234ms |     2048KB
✅ windows-x64      | x86_64-windows-gnu    |   1456ms |     2156KB
✅ macos-x64        | x86_64-macos          |   1321ms |     2089KB
------------------------------------
Summary: 3 successful, 0 failed
Total build time: 4011ms
Total package size: 6MB
Average build time: 1337ms
Packages available in: multi-arch-builds/
```

### Target Set Details

**Desktop Targets:**
```
linux-x64    x86_64-linux-gnu    ReleaseFast
windows-x64  x86_64-windows-gnu  ReleaseFast  
macos-x64    x86_64-macos        ReleaseFast
```

**Embedded Targets:**
```
arm64-linux   aarch64-linux-gnu      ReleaseSmall
arm32-linux   arm-linux-gnueabihf    ReleaseSmall
riscv64-linux riscv64-linux-gnu      ReleaseSmall
```

**Web Targets:**
```
wasm32-wasi         wasm32-wasi             ReleaseSmall
wasm32-freestanding wasm32-freestanding     ReleaseSmall
```

---

## Configuration & Cache

Cache management and configuration options.

### Cache Commands

```bash
# View cache statistics
reap cache-stats

# Clear all cache
reap clean --all

# Clear only build cache
reap clean --cache-only

# Set cache size limit
reap --cache-size=200 build  # 200MB limit
```

### Cache Statistics Output
```
==> Cache Statistics:
    Entries: 15
    Size: 87MB / 100MB
    Location: ~/.cache/reap
    Hit rate: 73% (22/30 builds)
    
Recent builds:
  hello-world-1.0.0    45MB    3 hits    2 days ago
  neovim-nightly       123MB   1 hit     1 week ago
```

### Configuration Files

**~/.config/reap/config.toml** (planned):
```toml
[cache]
max_size_mb = 200
cleanup_threshold = 0.8
compression_level = 6

[build]
parallel_downloads = 8
default_arch = ["x86_64"]
temp_dir = "/tmp/reap"

[aur]
cache_repos = true
auto_update = false
```

---

## Builder Commands

Advanced system building commands for kernels, drivers, and ISOs.

### `reap make`
Universal builder for kernels, drivers, and ISOs.

```bash
reap make <target> [OPTIONS]
```

**Targets:**
- `kernel` - Build custom Linux kernel with patches
- `ghostnv` - Build NVIDIA Ghost Open Driver
- `iso` - Create Arch-based ISO with custom stack

**Examples:**
```bash
reap make kernel --profile amd-x3d
reap make ghostnv --kernel linux-ghost
reap make iso --profile gaming --include ghostnv
```

### `reap forge`
Forge packages for AUR/Pacman deployment with style.

```bash
reap forge <target> [OPTIONS]
```

**Targets:**
- `package` - Build and deploy standard packages
- `weapon` - Performance-optimized packages  
- `armor` - Security-hardened packages
- `artifact` - Experimental/custom packages

**Examples:**
```bash
reap forge package mypackage --aur
reap forge weapon --name fastapp
reap forge armor --name secureapp
reap forge artifact --name experimental
```

**Philosophy:** Artisanal package creation with automatic AUR deployment.

### `reap build-system`
Production-ready system and package builds.

```bash
reap build-system <target> [OPTIONS]
```

**Targets:**
- `system` - Complete system with kernel and userspace
- `package` - Individual package builds
- `container` - OCI container images
- `module` - Kernel modules and drivers

**Examples:**
```bash
reap build-system system --profile server
reap build-system package firefox --optimize
reap build-system container ghost-base:latest
reap build-system module nvidia-ghost
```

**Philosophy:** Clean, reliable builds for production environments.

### `reap craft`
Artisanal kernel and driver crafting.

```bash
reap craft <target> [OPTIONS]
```

**Targets:**
- `kernel` - Build linux-ghost with custom patches
- `ghostnv` - Build GhostNV NVIDIA drivers
- `dkms` - Manage DKMS modules
- `patch` - Apply custom patches to kernels

**Examples:**
```bash
reap craft kernel --config x3d-gaming
reap craft ghostnv --kernel 6.15
reap craft dkms rebuild
reap craft patch bore-scheduler
```

**DKMS Actions:**
```bash
reap craft dkms add <module>      # Add DKMS module
reap craft dkms rebuild [module]  # Rebuild modules
reap craft dkms status           # Show module status
```

**Philosophy:** Carefully crafted custom kernels and drivers for enthusiasts.

---

## Examples & Workflows

### Basic PKGBUILD Workflow
```bash
# Start new package
reap init my-package
cd my-package

# Edit PKGBUILD file
vim PKGBUILD

# Test build
reap build

# Create final package
reap package

# Clean up
reap clean
```

### Modern reap.toml Workflow
```bash
# Initialize modern config
reap toml-init

# Edit configuration
vim reap.toml

# Build with AUR dependency resolution
reap toml-build

# Package is automatically created
```

### Cross-Compilation Workflow
```bash
# Detect project type
reap detect
# Output: Detected Zig project

# Build for current platform
reap compile --release

# Cross-compile for Windows
reap cross x86_64-windows-gnu --release

# Build for all desktop platforms
reap multi-build desktop --release
```

### AUR Development Workflow
```bash
# Search for dependencies
reap -Ss tree-sitter-cli

# Install build dependencies
reap -S tree-sitter-cli

# Create reap.toml with AUR deps
cat > reap.toml << EOF
[dependencies]
aur = ["tree-sitter-cli"]
EOF

# Build automatically resolves AUR deps
reap toml-build
```

### CI/CD Integration
```bash
#!/bin/bash
# .github/workflows/build.yml equivalent

# Install reap
curl -L https://github.com/ghostkellz/reap/releases/latest/download/reap -o reap
chmod +x reap

# Build for multiple platforms
./reap multi-build all --release

# Upload artifacts
tar -czf release-packages.tar.gz multi-arch-builds/
```

### Package Maintenance Workflow
```bash
# Update package version
sed -i 's/pkgver=.*/pkgver=2.0.0/' PKGBUILD

# Update checksums
curl -L https://example.com/source-2.0.0.tar.gz | sha256sum
# Update sha256sums in PKGBUILD

# Test build
zmake build

# Create signed package
zmake package --sign=maintainer@email.com

# Upload to repository
```

---

## Error Codes

reap uses standard exit codes:

```
0   Success
1   General error
2   Invalid command line arguments  
3   Configuration file error
4   Network/download error
5   Build/compilation error
6   Package creation error
7   Dependency resolution error
8   Cache error
9   Permission/filesystem error
```

## Exit Code Examples

```bash
# Check if build succeeded
reap build
if [ $? -eq 0 ]; then
    echo "Build successful"
else
    echo "Build failed with code $?"
fi

# Use in scripts
reap build || {
    echo "Build failed, cleaning up..."
    reap clean
    exit 1
}
```

---

## Compatibility Notes

### makepkg Compatibility
- **PKGBUILD format**: 100% compatible
- **Functions**: prepare(), build(), check(), package() all supported
- **Variables**: All standard variables supported
- **Arrays**: Bash array syntax fully supported
- **Environment**: Standard makepkg environment variables

### Differences from makepkg
- **Performance**: ~10x faster due to parallel processing
- **Caching**: Intelligent build caching (makepkg has none)
- **Cross-compilation**: Built-in (makepkg requires external tools)
- **Dependencies**: Parallel resolution (makepkg is sequential)
- **Modern config**: zmk.toml alternative (makepkg PKGBUILD only)

### Migration from makepkg
```bash
# Existing PKGBUILD works as-is
makepkg         # Old way
reap build      # New way, same result

# Enhanced workflow
reap toml-init           # Convert to modern format
reap toml-build          # Build with enhancements
reap multi-build all     # Cross-compile everything
```

---

For more detailed information, see [DOCS.md](DOCS.md) and the source code documentation.