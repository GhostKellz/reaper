#!/bin/bash
set -euo pipefail

# Reaper Uninstall Script v1.0
# Removes reaper binary and related files from the system

readonly SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly INSTALL_PREFIX="${INSTALL_PREFIX:-/usr/local}"
readonly COMPLETIONS_DIR="${COMPLETIONS_DIR:-/usr/share/zsh/site-functions}"
readonly VERSION="1.0"

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

show_version() {
    echo "Reaper Uninstall Script v${VERSION}"
    echo "Removes reaper binary and related files from the system"
    echo
}

show_help() {
    show_version
    echo "Usage: $0 [OPTIONS]"
    echo
    echo "Options:"
    echo "  --dry-run       Show what would be removed without actually removing"
    echo "  --keep-config   Keep configuration files (if any exist)"
    echo "  --force         Force removal even if files don't exist"
    echo "  --version       Show version information"
    echo "  --help          Show this help message"
    echo
    echo "Environment variables:"
    echo "  INSTALL_PREFIX     - Installation prefix (default: /usr/local)"
    echo "  COMPLETIONS_DIR    - Completions directory (default: /usr/share/zsh/site-functions)"
    echo
}

check_permissions() {
    log_info "Checking permissions..."
    
    # Check if we need sudo for the install prefix
    if [[ ! -w "$INSTALL_PREFIX/bin" ]] && [[ -d "$INSTALL_PREFIX/bin" ]]; then
        log_info "Root permissions required for $INSTALL_PREFIX/bin"
        if ! sudo -v; then
            log_error "Cannot obtain root permissions"
            exit 1
        fi
    fi
    
    # Check if we need sudo for completions
    if [[ ! -w "$COMPLETIONS_DIR" ]] && [[ -d "$COMPLETIONS_DIR" ]]; then
        log_info "Root permissions required for $COMPLETIONS_DIR"
        if ! sudo -v; then
            log_error "Cannot obtain root permissions"
            exit 1
        fi
    fi
    
    log_success "Permissions check completed"
}

find_reaper_installations() {
    log_info "Scanning for reaper installations..."
    
    local installations=()
    
    # Check common installation locations
    local locations=("/usr/local/bin" "/usr/bin" "/bin" "$HOME/.local/bin")
    
    for location in "${locations[@]}"; do
        if [[ -f "$location/reap" ]]; then
            installations+=("$location/reap")
        fi
    done
    
    # Check for symlinks
    local symlinks=("reaper" "reap-pkg")
    for location in "${locations[@]}"; do
        for link in "${symlinks[@]}"; do
            if [[ -L "$location/$link" ]]; then
                installations+=("$location/$link")
            fi
        done
    done
    
    # Check for completions
    if [[ -f "$COMPLETIONS_DIR/_reap" ]]; then
        installations+=("$COMPLETIONS_DIR/_reap")
    fi
    
    # Check for backup files
    for location in "${locations[@]}"; do
        if [[ -f "$location/reap.rust-backup" ]]; then
            installations+=("$location/reap.rust-backup")
        fi
    done
    
    if [[ ${#installations[@]} -eq 0 ]]; then
        log_warning "No reaper installations found"
        return 1
    fi
    
    log_info "Found installations:"
    for installation in "${installations[@]}"; do
        echo "  - $installation"
    done
    
    return 0
}

remove_files() {
    local dry_run="$1"
    local keep_config="$2"
    local force="$3"
    
    local files_to_remove=(
        "$INSTALL_PREFIX/bin/reap"
        "$INSTALL_PREFIX/bin/reaper"
        "$INSTALL_PREFIX/bin/reap-pkg"
        "$COMPLETIONS_DIR/_reap"
    )
    
    # Add backup files
    files_to_remove+=("$INSTALL_PREFIX/bin/reap.rust-backup")
    
    # Add potential config locations (if not keeping config)
    if [[ "$keep_config" != "true" ]]; then
        files_to_remove+=("$HOME/.config/reaper")
        files_to_remove+=("$HOME/.reaper")
    fi
    
    local removed_count=0
    
    for file in "${files_to_remove[@]}"; do
        if [[ -e "$file" ]] || [[ -L "$file" ]]; then
            if [[ "$dry_run" == "true" ]]; then
                log_info "Would remove: $file"
            else
                if [[ "$file" == "$HOME/.config/reaper" ]] || [[ "$file" == "$HOME/.reaper" ]]; then
                    # User files, don't need sudo
                    if rm -rf "$file"; then
                        log_success "Removed: $file"
                        ((removed_count++))
                    else
                        log_error "Failed to remove: $file"
                    fi
                else
                    # System files, might need sudo
                    if [[ -w "$(dirname "$file")" ]]; then
                        if rm -f "$file"; then
                            log_success "Removed: $file"
                            ((removed_count++))
                        else
                            log_error "Failed to remove: $file"
                        fi
                    else
                        if sudo rm -f "$file"; then
                            log_success "Removed: $file"
                            ((removed_count++))
                        else
                            log_error "Failed to remove: $file"
                        fi
                    fi
                fi
            fi
        elif [[ "$force" == "true" ]]; then
            if [[ "$dry_run" == "true" ]]; then
                log_info "Would try to remove: $file (not found)"
            else
                log_warning "File not found: $file"
            fi
        fi
    done
    
    if [[ "$dry_run" != "true" ]]; then
        log_info "Removed $removed_count files"
    fi
}

restore_backups() {
    local dry_run="$1"
    
    # Check for rust backup
    if [[ -f "$INSTALL_PREFIX/bin/reap.rust-backup" ]]; then
        if [[ "$dry_run" == "true" ]]; then
            log_info "Would restore rust backup: $INSTALL_PREFIX/bin/reap.rust-backup -> $INSTALL_PREFIX/bin/reap"
        else
            log_info "Found rust backup, restore it? [y/N]"
            read -r response
            if [[ "$response" =~ ^[Yy]$ ]]; then
                if [[ -w "$(dirname "$INSTALL_PREFIX/bin/reap")" ]]; then
                    mv "$INSTALL_PREFIX/bin/reap.rust-backup" "$INSTALL_PREFIX/bin/reap"
                else
                    sudo mv "$INSTALL_PREFIX/bin/reap.rust-backup" "$INSTALL_PREFIX/bin/reap"
                fi
                log_success "Restored rust version backup"
            else
                log_info "Keeping backup file"
            fi
        fi
    fi
}

verify_removal() {
    log_info "Verifying removal..."
    
    if command -v reap &> /dev/null; then
        local reap_location
        reap_location=$(command -v reap)
        log_warning "reap command still found at: $reap_location"
        log_warning "This might be a different installation or the PATH wasn't updated"
    else
        log_success "reap command no longer found in PATH"
    fi
}

main() {
    local dry_run="false"
    local keep_config="false"
    local force="false"
    
    # Parse command line arguments
    while [[ $# -gt 0 ]]; do
        case $1 in
            --dry-run)
                dry_run="true"
                shift
                ;;
            --keep-config)
                keep_config="true"
                shift
                ;;
            --force)
                force="true"
                shift
                ;;
            --version)
                show_version
                exit 0
                ;;
            --help|-h)
                show_help
                exit 0
                ;;
            *)
                log_error "Unknown option: $1"
                echo "Run '$0 --help' for usage information"
                exit 1
                ;;
        esac
    done
    
    echo
    echo "🗑️  Reaper Uninstall Script v${VERSION}"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo
    
    if [[ "$dry_run" == "true" ]]; then
        log_info "DRY RUN MODE - No files will be actually removed"
        echo
    fi
    
    # Check if running as root
    if [[ $EUID -eq 0 ]]; then
        log_error "Don't run this script as root"
        log_info "The script will ask for sudo when needed"
        exit 1
    fi
    
    # Find installations
    if ! find_reaper_installations; then
        if [[ "$force" == "true" ]]; then
            log_info "No installations found, but --force specified, continuing..."
        else
            log_info "Nothing to uninstall"
            exit 0
        fi
    fi
    
    echo
    
    # Confirm removal
    if [[ "$dry_run" != "true" ]]; then
        log_warning "This will remove reaper from your system"
        if [[ "$keep_config" == "true" ]]; then
            log_info "Configuration files will be preserved"
        else
            log_info "Configuration files will also be removed"
        fi
        echo
        log_info "Continue with removal? [y/N]"
        read -r response
        if [[ ! "$response" =~ ^[Yy]$ ]]; then
            log_info "Uninstall cancelled"
            exit 0
        fi
    fi
    
    # Check permissions
    check_permissions
    
    # Remove files
    echo
    log_info "Removing reaper files..."
    remove_files "$dry_run" "$keep_config" "$force"
    
    # Restore backups if requested
    if [[ "$dry_run" != "true" ]]; then
        echo
        restore_backups "$dry_run"
    fi
    
    # Verify removal
    if [[ "$dry_run" != "true" ]]; then
        echo
        verify_removal
    fi
    
    echo
    if [[ "$dry_run" == "true" ]]; then
        log_success "Dry run completed - no files were actually removed"
    else
        log_success "🎉 Reaper uninstall completed!"
        echo
        log_info "If you had shell completions enabled, you may need to restart your shell"
        log_info "or run 'hash -r' to update your command cache"
    fi
    echo
}

main "$@"