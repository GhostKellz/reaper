#!/bin/bash
# Reaper Uninstall Script

set -euo pipefail

INSTALL_DIR="/usr/local/bin"
CONFIG_DIR="${HOME}/.config/reap"
BINARY_NAME="reap"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

info() { echo -e "${GREEN}[INFO]${NC} $*"; }
warning() { echo -e "${YELLOW}[WARNING]${NC} $*"; }

echo "Reaper Uninstaller"
echo "=================="
echo ""

# Remove binary
if [[ -f "${INSTALL_DIR}/${BINARY_NAME}" ]]; then
    info "Removing binary..."
    sudo rm -f "${INSTALL_DIR}/${BINARY_NAME}"
    sudo rm -f "${INSTALL_DIR}/reaper"  # symlink if exists
    info "Binary removed"
else
    warning "Binary not found at ${INSTALL_DIR}/${BINARY_NAME}"
fi

# Ask about config
if [[ -d "${CONFIG_DIR}" ]]; then
    echo ""
    read -p "Remove configuration directory (${CONFIG_DIR})? [y/N] " -n 1 -r
    echo ""
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        rm -rf "${CONFIG_DIR}"
        info "Configuration removed"
    else
        info "Configuration preserved"
    fi
fi

echo ""
info "Reaper uninstalled successfully"
