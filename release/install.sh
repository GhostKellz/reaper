#!/bin/bash
# Reaper v0.6.0 Installation Script
# Universal installer for Arch-based systems with comprehensive error handling

set -euo pipefail

# Configuration
REPO_OWNER="GhostKellz"
REPO_NAME="reaper"
REPO_URL="https://github.com/${REPO_OWNER}/${REPO_NAME}"
INSTALL_DIR="/usr/local/bin"
CONFIG_DIR="${HOME}/.config/reap"
BINARY_NAME="reap"
VERSION="v0.6.0"

# Colors
readonly RED='\033[0;31m'
readonly GREEN='\033[0;32m'
readonly YELLOW='\033[1;33m'
readonly BLUE='\033[0;34m'
readonly PURPLE='\033[0;35m'
readonly NC='\033[0m'

# Logging
info() { echo -e "${BLUE}[INFO]${NC} $*"; }
success() { echo -e "${GREEN}[SUCCESS]${NC} $*"; }
warning() { echo -e "${YELLOW}[WARNING]${NC} $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*" >&2; }
debug() { [[ "${DEBUG:-0}" == "1" ]] && echo -e "${PURPLE}[DEBUG]${NC} $*" || true; }

# Cleanup on exit
cleanup() {
    local exit_code=$?
    if [[ -n "${TEMP_DIR:-}" && -d "${TEMP_DIR}" ]]; then
        debug "Cleaning up temporary directory: ${TEMP_DIR}"
        rm -rf "${TEMP_DIR}"
    fi
    exit $exit_code
}
trap cleanup EXIT INT TERM

# Check if command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Download with fallback
download() {
    local url="$1"
    local output="$2"
    
    if command_exists curl; then
        debug "Downloading with curl: $url"
        curl -fsSL --retry 3 --retry-delay 2 "$url" -o "$output"
    elif command_exists wget; then
        debug "Downloading with wget: $url"
        wget -q --tries=3 --timeout=20 "$url" -O "$output"
    else
        error "Neither curl nor wget found. Please install one of them."
        return 1
    fi
}

# Detect system architecture
detect_arch() {
    local arch
    arch=$(uname -m)
    case "$arch" in
        x86_64) echo "x86_64" ;;
        aarch64|arm64) echo "aarch64" ;;
        armv7l|armv7h) echo "armv7h" ;;
        *) 
            error "Unsupported architecture: $arch"
            return 1
            ;;
    esac
}

# Check if running on Arch-based system
check_system() {
    info "Checking system compatibility..."
    
    if [[ ! -f /etc/os-release ]]; then
        error "Cannot determine OS. /etc/os-release not found."
        return 1
    fi
    
    source /etc/os-release
    
    if [[ ! "$ID" =~ ^(arch|manjaro|endeavouros|garuda|artix)$ ]] && 
       [[ ! "$ID_LIKE" =~ arch ]]; then
        error "This installer is designed for Arch-based systems."
        error "Detected: ${NAME:-Unknown} (${ID:-unknown})"
        warning "You can still build from source: cargo install --git ${REPO_URL}"
        return 1
    fi
    
    success "Running on ${NAME} - compatible system"
}

# Check and install dependencies
check_dependencies() {
    info "Checking dependencies..."
    
    local missing_deps=()
    local optional_deps=()
    
    # Required dependencies
    if ! command_exists pacman; then
        error "pacman not found. This installer requires an Arch-based system."
        return 1
    fi
    
    # Build dependencies (if building from source)
    if [[ "${BUILD_FROM_SOURCE:-0}" == "1" ]]; then
        command_exists cargo || missing_deps+=("rust")
        command_exists git || missing_deps+=("git")
        command_exists gcc || missing_deps+=("base-devel")
    fi
    
    # Optional dependencies
    command_exists flatpak || optional_deps+=("flatpak")
    command_exists gpg || optional_deps+=("gnupg")
    
    # Report missing dependencies
    if [[ ${#missing_deps[@]} -gt 0 ]]; then
        error "Missing required dependencies: ${missing_deps[*]}"
        info "Install with: sudo pacman -S ${missing_deps[*]}"
        return 1
    fi
    
    if [[ ${#optional_deps[@]} -gt 0 ]]; then
        warning "Optional dependencies not found: ${optional_deps[*]}"
        info "For full functionality, install with: sudo pacman -S ${optional_deps[*]}"
    fi
    
    success "All required dependencies satisfied"
}

# Setup Flatpak if installed
setup_flatpak() {
    if ! command_exists flatpak; then
        debug "Flatpak not installed, skipping setup"
        return 0
    fi
    
    info "Setting up Flatpak integration..."
    
    # Check if Flathub is already added
    if ! flatpak remote-list | grep -q "^flathub"; then
        info "Adding Flathub repository..."
        flatpak remote-add --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo --user 2>/dev/null || {
            warning "Could not add Flathub repository. You may need to run with sudo or add it manually."
        }
    else
        debug "Flathub repository already configured"
    fi
    
    # Update Flatpak metadata
    info "Updating Flatpak metadata..."
    flatpak update --appstream 2>/dev/null || {
        warning "Could not update Flatpak metadata. Some features may not work properly."
    }
    
    success "Flatpak integration configured"
}

# Build from source
build_from_source() {
    info "Building Reaper from source..."
    
    TEMP_DIR=$(mktemp -d)
    cd "$TEMP_DIR"
    
    # Clone repository
    info "Cloning repository..."
    git clone --depth 1 --branch "${VERSION}" "${REPO_URL}" . || {
        warning "Could not clone specific version, trying main branch..."
        git clone --depth 1 "${REPO_URL}" .
    }
    
    # Build with optimizations
    info "Building with release optimizations..."
    cargo build --release --features cache || {
        error "Build failed"
        return 1
    }
    
    # Copy binary
    cp "target/release/${BINARY_NAME}" "${TEMP_DIR}/${BINARY_NAME}"
    
    success "Build completed successfully"
}

# Download pre-built binary
download_binary() {
    local arch
    arch=$(detect_arch)
    
    info "Attempting to download pre-built binary for ${arch}..."
    
    TEMP_DIR=$(mktemp -d)
    local binary_path="${TEMP_DIR}/${BINARY_NAME}"
    
    # Try different URL patterns
    local urls=(
        "${REPO_URL}/releases/download/${VERSION}/${BINARY_NAME}-${arch}"
        "${REPO_URL}/releases/download/${VERSION}/${BINARY_NAME}-${arch}-unknown-linux-gnu"
        "${REPO_URL}/releases/latest/download/${BINARY_NAME}-${arch}"
    )
    
    local download_success=0
    for url in "${urls[@]}"; do
        debug "Trying URL: $url"
        if download "$url" "$binary_path" 2>/dev/null; then
            if [[ -f "$binary_path" ]] && [[ -s "$binary_path" ]]; then
                download_success=1
                success "Downloaded binary from: $url"
                break
            fi
        fi
    done
    
    if [[ $download_success -eq 0 ]]; then
        warning "Pre-built binary not available for ${arch}"
        warning "Falling back to building from source..."
        BUILD_FROM_SOURCE=1
        build_from_source
    else
        chmod +x "$binary_path"
    fi
}

# Install binary
install_binary() {
    local binary_path="${TEMP_DIR}/${BINARY_NAME}"
    
    if [[ ! -f "$binary_path" ]]; then
        error "Binary not found at: $binary_path"
        return 1
    fi
    
    info "Installing binary to ${INSTALL_DIR}..."
    
    # Check if we need sudo
    if [[ -w "$INSTALL_DIR" ]]; then
        cp "$binary_path" "${INSTALL_DIR}/${BINARY_NAME}"
    else
        info "Elevated privileges required for system installation"
        sudo cp "$binary_path" "${INSTALL_DIR}/${BINARY_NAME}"
        sudo chmod 755 "${INSTALL_DIR}/${BINARY_NAME}"
    fi
    
    success "Binary installed to ${INSTALL_DIR}/${BINARY_NAME}"
}

# Install shell completions
install_completions() {
    info "Installing shell completions..."
    
    # Generate completions using the installed binary
    local comp_dir="${TEMP_DIR}/completions"
    mkdir -p "$comp_dir"
    
    # Try to generate completions
    if command_exists "${BINARY_NAME}"; then
        "${BINARY_NAME}" completions bash > "${comp_dir}/reap.bash" 2>/dev/null || true
        "${BINARY_NAME}" completions zsh > "${comp_dir}/_reap" 2>/dev/null || true
        "${BINARY_NAME}" completions fish > "${comp_dir}/reap.fish" 2>/dev/null || true
    fi
    
    # Install Bash completions
    if [[ -f "${comp_dir}/reap.bash" ]]; then
        for dir in "/usr/share/bash-completion/completions" \
                   "/etc/bash_completion.d" \
                   "${HOME}/.local/share/bash-completion/completions"; do
            if [[ -d "$dir" ]]; then
                if [[ -w "$dir" ]]; then
                    cp "${comp_dir}/reap.bash" "$dir/reap"
                else
                    sudo cp "${comp_dir}/reap.bash" "$dir/reap" 2>/dev/null || true
                fi
                success "Bash completions installed"
                break
            fi
        done
    fi
    
    # Install Zsh completions
    if [[ -f "${comp_dir}/_reap" ]]; then
        for dir in "/usr/share/zsh/site-functions" \
                   "/usr/local/share/zsh/site-functions" \
                   "${HOME}/.zsh/completions"; do
            if [[ -d "$dir" ]]; then
                if [[ -w "$dir" ]]; then
                    cp "${comp_dir}/_reap" "$dir/_reap"
                else
                    sudo cp "${comp_dir}/_reap" "$dir/_reap" 2>/dev/null || true
                fi
                success "Zsh completions installed"
                break
            fi
        done
    fi
    
    # Install Fish completions
    if [[ -f "${comp_dir}/reap.fish" ]]; then
        for dir in "/usr/share/fish/vendor_completions.d" \
                   "${HOME}/.config/fish/completions"; do
            if [[ -d "$dir" ]]; then
                if [[ -w "$dir" ]]; then
                    cp "${comp_dir}/reap.fish" "$dir/reap.fish"
                else
                    sudo cp "${comp_dir}/reap.fish" "$dir/reap.fish" 2>/dev/null || true
                fi
                success "Fish completions installed"
                break
            fi
        done
    fi
}

# Setup configuration
setup_config() {
    info "Setting up configuration..."
    
    # Create directories
    mkdir -p "${CONFIG_DIR}"/{profiles,hooks,taps}
    
    # Create main config if it doesn't exist
    if [[ ! -f "${CONFIG_DIR}/reap.toml" ]]; then
        cat > "${CONFIG_DIR}/reap.toml" << 'EOF'
# Reaper Configuration v0.6.0
backend_order = ["tap", "aur", "pacman", "flatpak"]
auto_resolve_deps = true
noconfirm = false
log_verbose = true
theme = "dark"
show_tips = true
enable_cache = true
enable_lua_hooks = false

[security]
strict_signatures = false
allow_insecure = false
gpg_keyserver = "hkps://keys.openpgp.org"

[performance]
parallel_jobs = 4
fast_mode = false
max_parallel = 8

[ui]
show_progress = true
colored_output = true
trust_badges = true
rating_stars = true

[flatpak]
enable = true
prefer_user = true
runtime_cleanup = true
EOF
        success "Configuration file created"
    else
        info "Configuration already exists, preserving existing settings"
    fi
    
    # Create sample hook
    local hook_file="${CONFIG_DIR}/hooks/post_install.sh"
    if [[ ! -f "$hook_file" ]]; then
        cat > "$hook_file" << 'EOF'
#!/bin/bash
# Example post-install hook
# Available environment variables:
# - REAP_PKG: Package name
# - REAP_VERSION: Package version
# - REAP_SOURCE: Installation source (pacman/aur/flatpak/tap)

if [[ -n "${REAP_PKG}" ]]; then
    echo "[HOOK] Successfully installed ${REAP_PKG} ${REAP_VERSION} from ${REAP_SOURCE}"
fi
EOF
        chmod +x "$hook_file"
        debug "Sample hook created"
    fi
}

# Create default profiles
create_profiles() {
    info "Creating default profiles..."
    
    local profiles_dir="${CONFIG_DIR}/profiles"
    
    # Developer profile
    if [[ ! -f "${profiles_dir}/developer.toml" ]]; then
        cat > "${profiles_dir}/developer.toml" << 'EOF'
name = "developer"
description = "Development environment with build tools"
backend_order = ["tap", "aur", "flatpak", "pacman"]
auto_install_deps = ["base-devel", "git", "rust", "nodejs", "python", "docker"]
pinned_packages = ["linux-lts", "linux-lts-headers"]
ignored_packages = []
parallel_jobs = 8
fast_mode = false
strict_signatures = true
auto_resolve_deps = true
prefer_binary = false
EOF
    fi
    
    # Gaming profile
    if [[ ! -f "${profiles_dir}/gaming.toml" ]]; then
        cat > "${profiles_dir}/gaming.toml" << 'EOF'
name = "gaming"
description = "Gaming-focused setup with performance optimizations"
backend_order = ["flatpak", "aur", "chaotic-aur", "pacman"]
auto_install_deps = ["steam", "lutris", "wine-staging", "gamemode", "mangohud"]
pinned_packages = ["nvidia-dkms", "lib32-nvidia-utils"]
ignored_packages = []
parallel_jobs = 6
fast_mode = true
strict_signatures = false
auto_resolve_deps = true
prefer_binary = true
EOF
    fi
    
    # Minimal profile
    if [[ ! -f "${profiles_dir}/minimal.toml" ]]; then
        cat > "${profiles_dir}/minimal.toml" << 'EOF'
name = "minimal"
description = "Minimal setup with only essential packages"
backend_order = ["pacman", "aur"]
auto_install_deps = []
pinned_packages = ["linux", "base", "base-devel"]
ignored_packages = ["*-git", "*-svn", "*-bzr"]
parallel_jobs = 2
fast_mode = true
strict_signatures = false
auto_resolve_deps = false
prefer_binary = true
EOF
    fi
    
    # Security profile
    if [[ ! -f "${profiles_dir}/security.toml" ]]; then
        cat > "${profiles_dir}/security.toml" << 'EOF'
name = "security"
description = "Security-focused profile with strict verification"
backend_order = ["pacman", "aur"]
auto_install_deps = ["gnupg", "firejail", "apparmor"]
pinned_packages = ["linux-hardened"]
ignored_packages = ["*-bin", "*-git"]
parallel_jobs = 2
fast_mode = false
strict_signatures = true
auto_resolve_deps = true
prefer_binary = false
allow_insecure = false
EOF
    fi
    
    success "Default profiles created (developer, gaming, minimal, security)"
}

# Verify installation
verify_installation() {
    info "Verifying installation..."
    
    # Check if binary exists and is executable
    if ! command_exists "${BINARY_NAME}"; then
        error "Installation verification failed. ${BINARY_NAME} not found in PATH."
        info "You may need to:"
        info "  1. Add ${INSTALL_DIR} to your PATH"
        info "  2. Restart your shell"
        info "  3. Run: export PATH=\"${INSTALL_DIR}:\$PATH\""
        return 1
    fi
    
    # Get version
    local installed_version
    installed_version=$("${BINARY_NAME}" --version 2>/dev/null | head -1) || {
        warning "Could not determine installed version"
        installed_version="unknown"
    }
    
    success "Reaper installed successfully: ${installed_version}"
    
    # Run basic health check
    info "Running basic health check..."
    if "${BINARY_NAME}" doctor --quiet 2>/dev/null; then
        success "Health check passed"
    else
        warning "Health check reported issues. Run 'reap doctor' for details."
    fi
    
    # Test Flatpak integration if available
    if command_exists flatpak && "${BINARY_NAME}" search --flatpak test 2>&1 | grep -q "Flatpak backend"; then
        success "Flatpak integration working"
    fi
}

# Print next steps
print_next_steps() {
    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    success "🎉 Reaper ${VERSION} installation completed!"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo ""
    info "🚀 Quick Start Guide:"
    echo ""
    echo "  ${GREEN}Basic Commands:${NC}"
    echo "    reap search <package>     # Search across all backends"
    echo "    reap install <package>    # Install a package"
    echo "    reap upgrade              # Upgrade all packages"
    echo "    reap tui                  # Launch interactive TUI"
    echo ""
    echo "  ${GREEN}Configuration:${NC}"
    echo "    reap profile list         # View available profiles"
    echo "    reap profile use gaming   # Switch to gaming profile"
    echo "    reap config edit          # Edit configuration"
    echo ""
    echo "  ${GREEN}Maintenance:${NC}"
    echo "    reap doctor              # Check system health"
    echo "    reap cache clean         # Clean package cache"
    echo "    reap orphans             # Find orphaned packages"
    echo ""
    echo "  ${GREEN}Resources:${NC}"
    echo "    Documentation: ${REPO_URL}"
    echo "    Configuration: ${CONFIG_DIR}/reap.toml"
    echo "    Report issues: ${REPO_URL}/issues"
    echo ""
    
    if [[ -n "${optional_deps[*]:-}" ]]; then
        warning "Remember to install optional dependencies for full functionality:"
        echo "    sudo pacman -S ${optional_deps[*]}"
    fi
}

# Main installation flow
main() {
    echo "☠️  Reaper Package Manager Installer"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo ""
    
    # Parse arguments
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --build|--source)
                BUILD_FROM_SOURCE=1
                info "Will build from source"
                ;;
            --debug)
                DEBUG=1
                debug "Debug mode enabled"
                ;;
            --version)
                VERSION="$2"
                shift
                ;;
            --help|-h)
                echo "Usage: $0 [OPTIONS]"
                echo ""
                echo "Options:"
                echo "  --build, --source    Build from source instead of downloading"
                echo "  --debug              Enable debug output"
                echo "  --version VERSION    Install specific version (default: ${VERSION})"
                echo "  --help, -h           Show this help message"
                exit 0
                ;;
            *)
                warning "Unknown option: $1"
                ;;
        esac
        shift
    done
    
    # Run installation steps
    check_system || exit 1
    check_dependencies || exit 1
    
    # Download or build binary
    if [[ "${BUILD_FROM_SOURCE:-0}" == "1" ]]; then
        build_from_source || exit 1
    else
        download_binary || exit 1
    fi
    
    install_binary || exit 1
    install_completions
    setup_config
    create_profiles
    setup_flatpak
    verify_installation || exit 1
    
    print_next_steps
}

# Run installer
main "$@"