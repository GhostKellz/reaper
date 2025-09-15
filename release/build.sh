#!/bin/bash
# Reaper Build Script for Release Packaging
# Creates release artifacts for v0.6.0

set -e

VERSION="0.6.0"
TARGET_DIR="target/release"
RELEASE_DIR="release/artifacts"
COMPLETION_DIR="completions"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m'

info() { echo -e "${BLUE}[BUILD]${NC} $1"; }
success() { echo -e "${GREEN}[SUCCESS]${NC} $1"; }
error() { echo -e "${RED}[ERROR]${NC} $1"; }

# Clean and prepare
prepare() {
    info "Preparing build environment..."
    rm -rf "$RELEASE_DIR"
    mkdir -p "$RELEASE_DIR"
    mkdir -p "$COMPLETION_DIR/bash"
    mkdir -p "$COMPLETION_DIR/zsh" 
    mkdir -p "$COMPLETION_DIR/fish"
}

# Build the binary
build() {
    info "Building Reaper v$VERSION..."
    
    # Build with optimizations
    cargo build --release --features cache
    
    # Run tests
    info "Running tests..."
    cargo test --release
    
    # Check binary size
    local size=$(du -h "$TARGET_DIR/reap" | cut -f1)
    success "Binary built successfully ($size)"
}

# Generate completions
generate_completions() {
    info "Generating shell completions..."
    
    # Generate completions (would need to implement in CLI)
    # For now, create placeholder files
    
    cat > "$COMPLETION_DIR/bash/reap" << 'EOF'
# Reaper bash completion
_reap() {
    local cur prev words cword
    _init_completion || return

    case "$prev" in
        install|remove|search|trust|profile|aur)
            # Package name completion would go here
            return 0
            ;;
        --backend)
            COMPREPLY=($(compgen -W "aur flatpak pacman tap" -- "$cur"))
            return 0
            ;;
    esac

    if [[ "$cur" == -* ]]; then
        COMPREPLY=($(compgen -W "--help --version --install --remove --search --backend --edit --diff --strict --insecure --fast" -- "$cur"))
        return 0
    fi

    COMPREPLY=($(compgen -W "install remove search upgrade clean doctor tui profile trust aur flatpak tap gpg config backup" -- "$cur"))
}

complete -F _reap reap
EOF

    cat > "$COMPLETION_DIR/zsh/_reap" << 'EOF'
#compdef reap

_reap() {
    local context state line
    typeset -A opt_args

    _arguments -C \
        '1: :_reap_commands' \
        '*::arg:->args'

    case $state in
        args)
            case $line[1] in
                install|remove|search)
                    _arguments '*:package:_reap_packages'
                    ;;
                profile)
                    _arguments '*:profile:_reap_profiles'
                    ;;
            esac
            ;;
    esac
}

_reap_commands() {
    local commands
    commands=(
        'install:Install a package'
        'remove:Remove a package'
        'search:Search for packages'
        'upgrade:Upgrade packages'
        'clean:Clean cache'
        'doctor:System health check'
        'tui:Launch interactive TUI'
        'profile:Manage profiles'
        'trust:Security analysis'
        'aur:AUR operations'
        'flatpak:Flatpak operations'
        'tap:Tap management'
        'gpg:GPG operations'
        'config:Configuration'
        'backup:Backup config'
    )
    _describe 'commands' commands
}

_reap_packages() {
    # Package completion would be implemented here
}

_reap_profiles() {
    local profiles
    profiles=($(reap profile list 2>/dev/null | tail -n +2))
    _describe 'profiles' profiles
}

_reap "$@"
EOF

    cat > "$COMPLETION_DIR/fish/reap.fish" << 'EOF'
# Reaper fish completion

# Main commands
complete -c reap -n "__fish_use_subcommand" -a "install" -d "Install a package"
complete -c reap -n "__fish_use_subcommand" -a "remove" -d "Remove a package"
complete -c reap -n "__fish_use_subcommand" -a "search" -d "Search for packages"
complete -c reap -n "__fish_use_subcommand" -a "upgrade" -d "Upgrade packages"
complete -c reap -n "__fish_use_subcommand" -a "clean" -d "Clean cache"
complete -c reap -n "__fish_use_subcommand" -a "doctor" -d "System health check"
complete -c reap -n "__fish_use_subcommand" -a "tui" -d "Launch interactive TUI"
complete -c reap -n "__fish_use_subcommand" -a "profile" -d "Manage profiles"
complete -c reap -n "__fish_use_subcommand" -a "trust" -d "Security analysis"
complete -c reap -n "__fish_use_subcommand" -a "aur" -d "AUR operations"
complete -c reap -n "__fish_use_subcommand" -a "flatpak" -d "Flatpak operations"
complete -c reap -n "__fish_use_subcommand" -a "tap" -d "Tap management"
complete -c reap -n "__fish_use_subcommand" -a "gpg" -d "GPG operations"
complete -c reap -n "__fish_use_subcommand" -a "config" -d "Configuration"
complete -c reap -n "__fish_use_subcommand" -a "backup" -d "Backup config"

# Global options
complete -c reap -l help -d "Show help"
complete -c reap -l version -d "Show version"
complete -c reap -l backend -xa "aur flatpak pacman tap" -d "Select backend"
complete -c reap -l edit -d "Edit PKGBUILD before building"
complete -c reap -l diff -d "Show PKGBUILD diff"
complete -c reap -l strict -d "Require GPG signatures"
complete -c reap -l insecure -d "Skip GPG verification"
complete -c reap -l fast -d "Fast mode"

# Profile subcommands
complete -c reap -n "__fish_seen_subcommand_from profile" -a "create list switch show delete edit" -d "Profile operations"

# Trust subcommands  
complete -c reap -n "__fish_seen_subcommand_from trust" -a "score scan stats update" -d "Trust operations"

# AUR subcommands
complete -c reap -n "__fish_seen_subcommand_from aur" -a "fetch edit deps" -d "AUR operations"
EOF

    success "Shell completions generated"
}

# Package for distribution
package() {
    info "Creating release packages..."
    
    # Copy binary
    cp "$TARGET_DIR/reap" "$RELEASE_DIR/reap-x86_64"
    chmod +x "$RELEASE_DIR/reap-x86_64"
    
    # Create tarball
    tar -czf "$RELEASE_DIR/reap-x86_64.tar.gz" \
        -C "$TARGET_DIR" reap \
        -C "../../$COMPLETION_DIR" bash zsh fish \
        -C "../../" README.md FEATURES.md SECURITY.md COMMANDS.md CHANGELOG.md LICENSE
    
    # Copy additional files
    cp release/PKGBUILD "$RELEASE_DIR/"
    cp release/install.sh "$RELEASE_DIR/"
    chmod +x "$RELEASE_DIR/install.sh"
    
    # Generate checksums
    cd "$RELEASE_DIR"
    sha256sum * > SHA256SUMS
    cd - > /dev/null
    
    success "Release packages created in $RELEASE_DIR/"
}

# Generate release notes
release_notes() {
    info "Generating release notes..."
    
    cat > "$RELEASE_DIR/RELEASE_NOTES.md" << EOF
# Reaper v$VERSION Release

🚀 **Advanced TUI with Real-time Monitoring**

## 🔥 What's New in v0.6.0

### 📊 Advanced TUI with Real-time Monitoring
- **Real-time package operation monitoring** with live status updates
- **Enhanced search interface** with backend filtering (AUR/Flatpak/Pacman)
- **Queue management system** for batch operations
- **Live log viewer** with color-coded output
- **System monitoring tab** showing disk usage, updates, and health

### 🔧 Improved Flatpak Integration
- **Better error handling** for Flatpak operations
- **Automatic Flathub repository setup** during installation
- **Non-interactive installation mode** for automation
- **Enhanced search parsing** for accurate results
- **Flatpak availability detection** before operations

### 🛠️ Enhanced Installation Experience
- **Robust installer script** with comprehensive error handling
- **Automatic fallback to source build** when binaries unavailable
- **Support for multiple architectures** (x86_64, aarch64, armv7h)
- **Improved dependency checking** with clear error messages
- **Test suite** for verifying installation integrity

### 🔒 Security & Profile Enhancements
- **New security profile template** with strict verification
- **Enhanced profile descriptions** for better understanding
- **Flatpak configuration section** in main config
- **Improved hook system** with environment variables

### 🐛 Bug Fixes
- Fixed Flatpak search result parsing issues
- Resolved installation errors with missing error handling
- Corrected doc comment syntax errors
- Improved error messages throughout the codebase

## 🏗️ Infrastructure Updates
- Updated CI/CD workflow with correct test logic
- Enhanced build script for multi-architecture support
- Comprehensive test suite for installation verification
- Improved release packaging process

## 📥 Installation

### Quick Install (Recommended)
\`\`\`bash
curl -sSL https://raw.githubusercontent.com/GhostKellz/reaper/main/release/install.sh | bash
\`\`\`

### Manual Install
1. Download \`reap-x86_64.tar.gz\`
2. Extract: \`tar -xzf reap-x86_64.tar.gz\`
3. Install: \`sudo cp reap /usr/local/bin/\`
4. Install completions (optional)

### From AUR
\`\`\`bash
# PKGBUILD included for AUR submission
makepkg -si
\`\`\`

## 🚀 Quick Start

\`\`\`bash
# Create and switch to developer profile
reap profile create dev --template developer
reap profile switch dev

# Install with trust analysis
reap install firefox
reap trust score firefox

# Rate a package
reap rate firefox 5 "Excellent browser!"

# Launch interactive TUI
reap tui

# System health check
reap doctor
\`\`\`

## 🔧 Breaking Changes
- Profile system replaces simple config options
- Trust analysis now runs by default (can be disabled)
- TUI interface completely redesigned

## 📚 Documentation
- [Features Guide](FEATURES.md)
- [Security Guide](SECURITY.md)  
- [Commands Reference](COMMANDS.md)
- [API Documentation](API.md)

**Full Changelog**: [CHANGELOG.md](CHANGELOG.md)
EOF

    success "Release notes generated"
}

# Main build process
main() {
    echo "🔥 Reaper v$VERSION Release Builder"
    echo "==================================="
    
    prepare
    build
    generate_completions  
    package
    release_notes
    
    echo ""
    success "🎉 Release build completed successfully!"
    echo ""
    info "Release artifacts:"
    ls -la "$RELEASE_DIR/"
    echo ""
    info "Ready for distribution!"
}

# Run if called directly
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    main "$@"
fi