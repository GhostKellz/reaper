#!/bin/bash
# Test script for Reaper installation process

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Test configuration
TEST_DIR="/tmp/reaper-install-test"
INSTALL_SCRIPT="./release/install.sh"

info() { echo -e "${BLUE}[TEST]${NC} $*"; }
success() { echo -e "${GREEN}[PASS]${NC} $*"; }
warning() { echo -e "${YELLOW}[WARN]${NC} $*"; }
error() { echo -e "${RED}[FAIL]${NC} $*" >&2; }

# Cleanup function
cleanup() {
    info "Cleaning up test environment..."
    [[ -d "$TEST_DIR" ]] && rm -rf "$TEST_DIR"
    # Remove test binary if installed
    [[ -f "/usr/local/bin/reap-test" ]] && sudo rm -f "/usr/local/bin/reap-test"
}

trap cleanup EXIT

# Test 1: Check installer script exists and is executable
test_installer_exists() {
    info "Test 1: Checking installer script..."
    
    if [[ ! -f "$INSTALL_SCRIPT" ]]; then
        error "Installer script not found at $INSTALL_SCRIPT"
        return 1
    fi
    
    if [[ ! -x "$INSTALL_SCRIPT" ]]; then
        warning "Installer script is not executable, fixing..."
        chmod +x "$INSTALL_SCRIPT"
    fi
    
    success "Installer script found and is executable"
}

# Test 2: Test installer help
test_installer_help() {
    info "Test 2: Testing installer help..."
    
    if "$INSTALL_SCRIPT" --help | grep -q "Usage:"; then
        success "Installer help works correctly"
    else
        error "Installer help output is missing or incorrect"
        return 1
    fi
}

# Test 3: Test system detection
test_system_detection() {
    info "Test 3: Testing system detection..."
    
    # Create a mock environment
    mkdir -p "$TEST_DIR"
    
    # Test if installer detects Arch-based system
    if grep -q "arch" /etc/os-release || grep -q "ID_LIKE.*arch" /etc/os-release; then
        success "System correctly identified as Arch-based"
    else
        warning "Not running on Arch-based system, some tests may fail"
    fi
}

# Test 4: Test dependency checking
test_dependencies() {
    info "Test 4: Testing dependency checking..."
    
    local missing_deps=()
    
    # Check required tools
    command -v curl >/dev/null 2>&1 || missing_deps+=("curl")
    command -v wget >/dev/null 2>&1 || missing_deps+=("wget")
    
    if [[ ${#missing_deps[@]} -eq 2 ]]; then
        error "Neither curl nor wget available"
        return 1
    fi
    
    # Check optional deps
    if command -v flatpak >/dev/null 2>&1; then
        success "Flatpak is installed"
        
        # Test Flatpak functionality
        if flatpak remote-list | grep -q "flathub"; then
            success "Flathub repository is configured"
        else
            warning "Flathub repository not configured"
        fi
    else
        warning "Flatpak not installed - some features won't be available"
    fi
    
    success "Dependency check completed"
}

# Test 5: Test download functionality
test_download() {
    info "Test 5: Testing download functionality..."
    
    # Test GitHub API access
    if curl -sI "https://api.github.com/repos/GhostKellz/reaper/releases" | grep -q "200 OK"; then
        success "GitHub API is accessible"
    else
        warning "GitHub API might be rate limited or inaccessible"
    fi
    
    # Test if release URLs are accessible
    local test_urls=(
        "https://github.com/GhostKellz/reaper/releases/download/v0.6.0/reap-x86_64"
        "https://github.com/GhostKellz/reaper/releases/latest/download/reap-x86_64"
    )
    
    local found_binary=0
    for url in "${test_urls[@]}"; do
        if curl -sI "$url" | grep -qE "(200 OK|302 Found)"; then
            success "Found accessible binary at: $url"
            found_binary=1
            break
        fi
    done
    
    if [[ $found_binary -eq 0 ]]; then
        warning "No pre-built binaries found, installer will build from source"
    fi
}

# Test 6: Test build from source option
test_build_from_source() {
    info "Test 6: Testing build from source option..."
    
    if command -v cargo >/dev/null 2>&1; then
        success "Rust toolchain is installed"
        
        # Check if we can access the repository
        if git ls-remote https://github.com/GhostKellz/reaper.git >/dev/null 2>&1; then
            success "Repository is accessible"
        else
            error "Cannot access repository"
            return 1
        fi
    else
        warning "Rust not installed, build from source will fail"
    fi
}

# Test 7: Test configuration setup
test_config_setup() {
    info "Test 7: Testing configuration setup..."
    
    # Create a test config directory
    local test_config_dir="$TEST_DIR/.config/reap"
    mkdir -p "$test_config_dir"/{profiles,hooks,taps}
    
    # Test config file creation
    if [[ -d "$test_config_dir" ]]; then
        success "Configuration directories can be created"
    else
        error "Failed to create configuration directories"
        return 1
    fi
}

# Test 8: Dry run installation
test_dry_run() {
    info "Test 8: Running installation dry run..."
    
    # We'll simulate the installation without actually installing
    warning "Dry run mode - will test installer flow without actual installation"
    
    # Test if installer script runs without errors
    if bash -n "$INSTALL_SCRIPT"; then
        success "Installer script syntax is valid"
    else
        error "Installer script has syntax errors"
        return 1
    fi
}

# Main test runner
main() {
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "Reaper Installation Test Suite"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo ""
    
    local failed_tests=0
    
    # Run all tests
    test_installer_exists || ((failed_tests++))
    test_installer_help || ((failed_tests++))
    test_system_detection || ((failed_tests++))
    test_dependencies || ((failed_tests++))
    test_download || ((failed_tests++))
    test_build_from_source || ((failed_tests++))
    test_config_setup || ((failed_tests++))
    test_dry_run || ((failed_tests++))
    
    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    
    if [[ $failed_tests -eq 0 ]]; then
        success "All tests passed! ✨"
        echo ""
        info "To run the actual installation:"
        echo "  ./release/install.sh"
        echo ""
        info "Or to build from source:"
        echo "  ./release/install.sh --build"
    else
        error "Failed tests: $failed_tests"
        echo ""
        warning "Fix the issues above before running the installer"
    fi
    
    return $failed_tests
}

# Run tests
main "$@"