#!/bin/bash
set -euo pipefail

# Reaper Installation Script
# Build and install the Zig version of reaper

readonly SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly INSTALL_PREFIX="${INSTALL_PREFIX:-/usr/local}"
readonly COMPLETIONS_DIR="${COMPLETIONS_DIR:-/usr/share/zsh/site-functions}"

# Colors for output
readonly RED='\033[0;31m'
readonly GREEN='\033[0;32m'
readonly YELLOW='\033[1;33m'
readonly BLUE='\033[0;34m'
readonly NC='\033[0m' # No Color

log_info() {
    echo -e "${BLUE}:: ${*}${NC}"
}

log_success() {
    echo -e "${GREEN}✅ ${*}${NC}"
}

log_warning() {
    echo -e "${YELLOW}⚠️  ${*}${NC}"
}

log_error() {
    echo -e "${RED}❌ ${*}${NC}"
}

check_dependencies() {
    log_info "Checking dependencies..."
    
    local missing_deps=()
    
    # Check for Zig
    if ! command -v zig &> /dev/null; then
        missing_deps+=("zig")
    fi
    
    # Check for git (for AUR functionality)
    if ! command -v git &> /dev/null; then
        missing_deps+=("git")
    fi
    
    # Check for pacman (we're on Arch, right?)
    if ! command -v pacman &> /dev/null; then
        log_warning "pacman not found - you may be on a non-Arch system"
    fi
    
    if [[ ${#missing_deps[@]} -gt 0 ]]; then
        log_error "Missing dependencies: ${missing_deps[*]}"
        log_info "Please install missing dependencies:"
        
        for dep in "${missing_deps[@]}"; do
            case $dep in
                zig)
                    echo "  - Arch Linux: pacman -S zig"
                    echo "  - Manual: https://ziglang.org/download/"
                    ;;
                git)
                    echo "  - pacman -S git"
                    ;;
            esac
        done
        
        exit 1
    fi
    
    log_success "All dependencies found"
}

check_zig_version() {
    log_info "Checking Zig version..."
    
    local zig_version
    zig_version=$(zig version)
    
    # Check if Zig version is compatible (0.15.0-dev+)
    if ! echo "$zig_version" | grep -qE '^0\.(15|1[6-9]|[2-9][0-9])\.'; then
        log_warning "Zig version $zig_version may not be compatible"
        log_warning "Required: Zig 0.15.0-dev or later for v2.0 features"
        
        read -p "Continue anyway? [y/N] " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            exit 1
        fi
    fi
    
    log_success "Zig version: $zig_version"
}

build_reaper() {
    log_info "Building reaper..."
    
    cd "$SCRIPT_DIR"
    
    # Clean previous builds
    if [[ -d "zig-out" ]]; then
        rm -rf zig-out
        log_info "Cleaned previous build"
    fi
    
    # Build in release mode
    log_info "Compiling with Zig (release mode)..."
    if ! zig build -Doptimize=ReleaseFast; then
        log_error "Build failed"
        exit 1
    fi
    
    # Verify the binary was created
    if [[ ! -f "zig-out/bin/reap" ]]; then
        log_error "Binary not found after build"
        exit 1
    fi
    
    log_success "Build completed successfully"
}

test_binary() {
    log_info "Testing binary..."
    
    cd "$SCRIPT_DIR"
    
    # Test basic functionality
    if ! ./zig-out/bin/reap help &> /dev/null; then
        log_error "Basic functionality test failed"
        exit 1
    fi
    
    # Test version command (v2.0 specific)  
    if ! ./zig-out/bin/reap version 2>&1 | grep -q "2.2.0"; then
        log_error "Version test failed - not v2.2.0"
        log_error "Actual output: $(./zig-out/bin/reap version 2>&1 | head -1)"
        exit 1
    fi
    
    # Test trust command (new in v2.0)
    if ! ./zig-out/bin/reap trust --help &> /dev/null 2>&1; then
        log_warning "Trust command test failed (may not be critical)"
    fi
    
    log_success "Binary tests passed"
}

install_binary() {
    log_info "Installing binary to $INSTALL_PREFIX/bin..."
    
    # Create install directory if it doesn't exist
    sudo mkdir -p "$INSTALL_PREFIX/bin"
    
    # Install the binary
    sudo cp "$SCRIPT_DIR/zig-out/bin/reap" "$INSTALL_PREFIX/bin/"
    sudo chmod 755 "$INSTALL_PREFIX/bin/reap"
    
    log_success "Binary installed to $INSTALL_PREFIX/bin/reap"
}

install_completions() {
    log_info "Installing zsh completions..."
    
    if [[ -f "$SCRIPT_DIR/completions/_reap" ]]; then
        # Create completions directory if it doesn't exist
        sudo mkdir -p "$COMPLETIONS_DIR"
        
        # Install completions
        sudo cp "$SCRIPT_DIR/completions/_reap" "$COMPLETIONS_DIR/"
        
        log_success "Zsh completions installed to $COMPLETIONS_DIR/_reap"
        log_info "Restart your shell or run 'autoload -U compinit && compinit' to enable completions"
    else
        log_warning "Completions file not found, skipping"
    fi
}

create_symlinks() {
    log_info "Creating compatibility symlinks..."
    
    # Create symlinks for common package manager names
    local symlinks=("reaper" "reap-pkg")
    
    for link in "${symlinks[@]}"; do
        if [[ ! -e "$INSTALL_PREFIX/bin/$link" ]]; then
            sudo ln -s "$INSTALL_PREFIX/bin/reap" "$INSTALL_PREFIX/bin/$link"
            log_success "Created symlink: $link -> reap"
        else
            log_warning "Symlink $link already exists, skipping"
        fi
    done
}

backup_rust_version() {
    log_info "Checking for existing rust reaper installation..."
    
    # Check if rust version is installed
    if command -v reap &> /dev/null; then
        local existing_reap
        existing_reap=$(command -v reap)
        
        # Check if it's the rust version (simple heuristic)
        if file "$existing_reap" | grep -q "ELF.*dynamically linked"; then
            log_warning "Found existing reaper installation: $existing_reap"
            
            read -p "Backup existing installation? [Y/n] " -n 1 -r
            echo
            if [[ ! $REPLY =~ ^[Nn]$ ]]; then
                sudo mv "$existing_reap" "${existing_reap}.rust-backup"
                log_success "Backed up to ${existing_reap}.rust-backup"
            fi
        fi
    fi
}

verify_installation() {
    log_info "Verifying installation..."
    
    # Check if reap command is available
    if ! command -v reap &> /dev/null; then
        log_error "reap command not found in PATH"
        log_info "You may need to add $INSTALL_PREFIX/bin to your PATH"
        exit 1
    fi
    
    # Test basic functionality
    if ! reap help &> /dev/null; then
        log_error "reap command not working properly"
        exit 1
    fi
    
    log_success "Installation verified successfully"
}

show_completion_message() {
    echo
    log_success "🎉 Reaper v2.2.0 (Zig version) installed successfully!"
    echo
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo
    echo "🚀 Quick Start:"
    echo "   reap search firefox      # Search for packages with trust badges"
    echo "   reap install yay-bin     # Install AUR packages with security analysis"
    echo "   reap info firefox        # Show detailed package information"
    echo "   reap trust firefox       # Comprehensive trust & security analysis"
    echo "   reap --tui               # Launch modern Phantom TUI interface"
    echo
    echo "📚 v2.0 New Features:"
    echo "   🛡️  Trust Scoring       - Every package gets 0-10 trust score"
    echo "   🔍 Security Analysis    - 30+ patterns detect suspicious code" 
    echo "   🔐 GPG Verification     - Auto-import keys and verify signatures"
    echo "   🎨 Phantom TUI          - Modern terminal interface"
    echo "   ⚡ zsync Async Runtime  - High-performance parallel operations"
    echo
    echo "📊 Trust Badges:"
    echo "   ⭐ 8.0-10.0 Excellent   ✓ 6.0-7.9 Good   ? 4.0-5.9 Fair   ⚠ <4.0 Low"
    echo
    echo "🔧 Compatibility:"
    echo "   All pacman flags work: reap -S package, reap -Syu, etc."
    echo "   Symlinks available: reaper, reap-pkg"
    echo
    if [[ -f "${COMPLETIONS_DIR}/_reap" ]]; then
        echo "⚡ Tab completion: Restart your shell to enable zsh completions"
        echo
    fi
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo
}

main() {
    echo
    echo "🔨 Reaper v2.2.0 Installation Script"
    echo "   Trust Scoring • Security Analysis • Phantom TUI • zsync Runtime"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo
    
    # Check if running as root
    if [[ $EUID -eq 0 ]]; then
        log_error "Don't run this script as root"
        log_info "The script will ask for sudo when needed"
        exit 1
    fi
    
    # Run installation steps
    check_dependencies
    check_zig_version
    backup_rust_version
    build_reaper
    test_binary
    install_binary
    install_completions
    create_symlinks
    verify_installation
    show_completion_message
}

# Handle command line arguments
case "${1:-install}" in
    install)
        main
        ;;
    build-only)
        check_dependencies
        check_zig_version
        build_reaper
        test_binary
        log_success "Build completed. Binary available at: $SCRIPT_DIR/zig-out/bin/reap"
        ;;
    clean)
        log_info "Cleaning build artifacts..."
        cd "$SCRIPT_DIR"
        rm -rf zig-out .zig-cache
        log_success "Clean completed"
        ;;
    uninstall)
        log_info "Uninstalling reaper..."
        sudo rm -f "$INSTALL_PREFIX/bin/reap"
        sudo rm -f "$INSTALL_PREFIX/bin/reaper"
        sudo rm -f "$INSTALL_PREFIX/bin/reap-pkg"
        sudo rm -f "$COMPLETIONS_DIR/_reap"
        log_success "Uninstall completed"
        ;;
    --help|-h)
        echo "Usage: $0 [install|build-only|clean|uninstall|--help]"
        echo
        echo "Commands:"
        echo "  install     - Build and install reaper (default)"
        echo "  build-only  - Only build, don't install"
        echo "  clean       - Clean build artifacts"
        echo "  uninstall   - Remove installed files"
        echo "  --help      - Show this help"
        echo
        echo "Environment variables:"
        echo "  INSTALL_PREFIX     - Installation prefix (default: /usr/local)"
        echo "  COMPLETIONS_DIR    - Completions directory (default: /usr/share/zsh/site-functions)"
        ;;
    *)
        log_error "Unknown command: $1"
        echo "Run '$0 --help' for usage information"
        exit 1
        ;;
esac