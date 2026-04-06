# Quick Start

Get started with Reaper in under 5 minutes.

## Prerequisites

- Arch Linux (or derivative)
- Rust toolchain (for building from source)
- `base-devel` package group
- GPG (for signature verification)

## Install

```bash
# Build from source
git clone https://github.com/GhostKellz/reaper.git
cd reaper
cargo build --release
cargo install --path .
```

Or use the install script:

```bash
curl -sSL https://raw.githubusercontent.com/GhostKellz/reaper/main/release/install.sh | bash
```

## Basic Usage

```bash
# Search for packages
reap search firefox

# Install a package
reap install firefox

# Install with pacman-style flags
reap -S firefox

# Upgrade all packages
reap -Syu

# Remove a package
reap remove firefox
# or
reap -R firefox
```

## Pacman Compatibility

Reaper supports familiar pacman-style flags:

| Flag | Description |
|------|-------------|
| `-S <pkg>` | Install package |
| `-R <pkg>` | Remove package |
| `-Ss <term>` | Search packages |
| `-Syu` | Sync and upgrade all |
| `-Sy` | Sync package database |
| `-Su` | Upgrade packages |
| `-Qu` | List upgradable packages |
| `-Sc` | Clean cache |

## Interactive TUI

Launch the terminal user interface:

```bash
reap tui
```

## System Health Check

Run diagnostics:

```bash
reap doctor
```

## Next Steps

- [Full Installation Guide](./installation.md)
- [Command Reference](../usage/commands.md)
- [Configuration](../usage/configuration.md)
