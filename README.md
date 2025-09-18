<p align="center">
  <img src="assets/reaper-logo.png" alt="Reaper Logo" width="200">
</p>

[![Arch Linux](https://img.shields.io/badge/platform-Arch%20Linux-1793d1?logo=arch-linux&logoColor=white)](https://archlinux.org)
[![Made with Rust](https://img.shields.io/badge/made%20with-Rust-000000?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Status](https://img.shields.io/badge/status-active-success?style=flat-square)](https://github.com/GhostKellz/reaper)
[![Build](https://img.shields.io/github/actions/workflow/status/GhostKellz/reaper/main.yml?branch=main)](https://github.com/GhostKellz/reaper/actions)
![Built with Clap](https://img.shields.io/badge/built%20with-clap-orange)
![License](https://img.shields.io/github/license/GhostKellz/reaper)

# ☠️ Reaper Package Manager

---

## 📄 Overview

**Reaper** is a blazing-fast, Rust-powered **AUR helper and meta package manager** for Arch Linux. It is designed for paranoid Arch users, power packagers, and automation-first workflows.

---

## 🔧 Capabilities

* Unified search: AUR, Pacman, Flatpak, ChaoticAUR, ghostctl-aur, custom binary repos
* Interactive TUI installer with multi-source search and install queue
* GPG key importing, PKGBUILD diff and auditing, trust level reporting
* Rollback system, backup/restore, and multi-package upgrades
* Flatpak integration with metadata and audit
* Orphan detection and removal (AUR/pacman)
* Dependency/conflict resolution with --resolve-deps
* Binary-only and repo selection (e.g. --repo=ghostctl-aur)
* Lua-configurable backend logic and plugin/hook support (planned)
* Search and PKGBUILD caching for speed
* **No yay/paru fallback** – all install/upgrade logic is native async Rust

---

## 📦 Installation

### 🔥 Quick Install (Recommended)

```bash
curl -sSL https://raw.githubusercontent.com/GhostKellz/reaper/main/release/install.sh | bash
```

### Build from Source

```bash
cargo install --path .
```

### AUR (planned)

```bash
yay -S reaper-bin
```

---

## 🚀 Usage

```bash
reap install <pkg> --fast           # Fast mode: skip signature, diff, dep tree
reap install <pkg> --strict         # Require GPG signature, abort if missing
reap install <pkg> --insecure       # Skip all GPG checks (not recommended)
reap install <pkg> --repo=ghostctl-aur  # Force binary repo
reap install <pkg> --binary-only        # Only install from binary repo
reap upgrade                        # Upgrade all packages
reap rollback <pkg>                 # Rollback a package
reap orphan [--remove]              # List/remove orphaned AUR/pacman packages
reap backup                         # Backup config
reap diff <pkg>                     # Show PKGBUILD diff before install/upgrade
reap pin <pkg>                      # Pin a package/version
reap clean                          # Clean cache
reap doctor                         # System/config health check
reap tui                            # Interactive TUI
reap gpg ...                        # GPG key management
reap flatpak ...                    # Flatpak management
reap tap ...                        # Tap repo management
```

- All install/upgrade flows are now async/parallel and do not use yay/paru fallback.
- TUI and CLI support all major commands, including rollback, pin, audit, and tap management.
- See [COMMANDS.md](COMMANDS.md) for the full updated command list.

### Tap GPG Verification Example

```bash
reap install <pkg> --strict
# Aborts if PKGBUILD.sig is missing or invalid
reap install <pkg> --insecure
# Skips all GPG checks
```

### Conflict Resolution and Rollback Example

```bash
reap install <pkg>
# If conflict: ⚠️ Conflict: /usr/bin/foo is owned by pacman:foo. Use --force to override.
reap rollback <pkg>
# Restores previous version from backup
```

---

## 🚀 Enhanced Usage Examples

```bash
# 🔍 Enhanced package operations with trust and ratings
reap install firefox --diff          # Show PKGBUILD diff before install
reap trust score firefox             # Check package security score
reap rate firefox 5 "Great browser!" # Rate with stars and comment

# 👤 Profile management for different workflows
reap profile create dev --template developer
reap profile switch gaming
reap profile show dev

# 🔧 Advanced AUR operations
reap aur fetch yay                    # Get PKGBUILD for analysis
reap aur edit custom-package          # Interactive PKGBUILD editing
reap aur deps firefox --conflicts     # Advanced dependency checking

# 📋 Interactive TUI with live monitoring
reap tui                              # Launch enhanced TUI
# Hotkeys: t=trust, r=rate, d=diff, p=profile, TAB=details

# 🛡️ Security and trust operations
reap trust scan                       # Scan all packages for security
reap trust stats                      # Show trust statistics

# Standard package operations (enhanced with trust/ratings)
reap search firefox                   # Search with trust badges
reap install firefox                  # Install with security checks
reap remove firefox                   # Interactive removal confirmation
reap upgrade                          # System upgrade with progress
```

---

## 🔐 Secure Publisher Verification

Reaper ensures that all tap-based packages are cryptographically verified before install (unless you use `--insecure`).

- **PKGBUILD.sig**: Each tap package must include a GPG signature for its PKGBUILD file.
- **publisher.toml**: Each tap must provide a `publisher.toml` with publisher info and GPG key fingerprint.
- **Verification flow:**
  1. On install, Reaper checks for `PKGBUILD.sig` and verifies it using the publisher's GPG key.
  2. If the key is missing, Reaper will auto-fetch it from a keyserver (configurable with `--gpg-keyserver`).
  3. If verification fails, install is aborted unless `--insecure` is passed.
  4. Publisher info and verification status are shown in the CLI and TUI.

**publisher.toml example:**
```toml
name = "GhostKellz"
gpg_key = "F7C2 0EFD 6F3E 9A88 F14A  77F3 CDEE 9E44 E881 E42E"
email = "ckelley@ghostkellz.sh"
verified = true
url = "https://ghostkellz.sh"
```

**For tap publishers:**
- Generate a GPG key (see PUBLISHING.md).
- Sign your PKGBUILD: `gpg --detach-sign --armor PKGBUILD`
- Add your info to publisher.toml and commit both files to your tap repo.

**For users:**
- Use `reap install <pkg>` as normal. Reaper will verify the signature and show publisher info.
- Use `--insecure` to skip verification (not recommended).

See [Full Docs](DOCS.md#publisher-verification-and-gpg) for details.

---

## ⚠️ Install Conflict Handling

Before installing, Reap checks for file conflicts using `pacman -Qo` on all files to be installed. If a conflict is found:

```
⚠️ Conflict: /usr/bin/foo is owned by pacman:foo. Use --force to override.
```

You can use `--force` to override, but this is not recommended unless you know what you are doing.

---

## 🔙 Rollback and Backup System

Before every install, Reap backs up the current package state (pacman db and binaries) to `~/.local/share/reap/backup/<pkg>/<timestamp>/`.

To rollback to the last good version:

```bash
reap rollback <pkg>
```

This restores the previous version from backup.

---

## 🔍 Smart Search with Tap Priorities

Reap searches all enabled taps first, respecting per-tap priority.

Use `reap tap set-priority ghost 10` to boost priority.
Results are merged with AUR/Flatpak, and sorted accordingly.

Enable search caching in `reap.toml` to avoid rate limits and speed up lookups.

---

## 🔁 Tap Auto-Sync

Reap automatically syncs all enabled taps before running search/install operations.

You can configure sync behavior in `~/.config/reap/reap.toml`:

```toml
[settings]
auto_sync = true
sync_interval_hours = 6
```

You can also manually run:

```bash
reap tap sync
```

---

## ⚙️ Config CLI

Update `reap.toml` without editing the file directly:

```bash
reap config set backend aur
reap config get backend
reap config show
```

Config precedence: CLI flag > `~/.config/reap/reap.toml` > default. Config is validated on load and errors will abort with a clear message.

---

## ### ⚙️ Smart Dependency Resolution

Reap resolves and installs dependencies from taps, AUR, or your system — no yay/paru needed. Enable `--resolve-deps` to automatically satisfy PKGBUILD or tap metadata dependencies.

Resolution order:
1. Tap packages (highest priority)
2. AUR packages (fallback)
3. Installed system packages (skipped)

---

## 🤖 Hooks and Automation

Reap supports shell-based hooks for automation. Place executable `.sh` scripts in `~/.config/reap/hooks/` (e.g., `pre_install.sh`, `post_install.sh`).

Each script receives context via environment variables:
- `REAP_PKG`, `REAP_VERSION`, `REAP_SOURCE`, `REAP_INSTALL_PATH`, `REAP_TAP`

Example:
```sh
echo "[HOOK] Installing $REAP_PKG from $REAP_SOURCE"
```

> Advanced: Lua scripting for hooks is planned as a future, low-priority feature for advanced users.

---

## 📂 Config Example

See [Full Docs](DOCS.md#configuration) for advanced configuration and Lua config examples.

---

## 📚 Documentation

### User Documentation
- **[Features Guide](FEATURES.md)** - Comprehensive feature overview
- **[Security Guide](SECURITY.md)** - Security features and best practices
- **[Profile Management](docs/profiles.md)** - Multi-profile system guide
- **[Trust System](docs/trust.md)** - Package trust and security analysis
- **[Interactive Features](docs/interactive.md)** - Rating system and prompts
- **[TUI Guide](docs/tui.md)** - Enhanced terminal user interface

### Developer Documentation
- **[API Reference](API.md)** - Complete API documentation
- **[Architecture](ARCHITECTURE.md)** - System design and structure
- **[Contributing](CONTRIBUTING.md)** - Development and contribution guide
- **[Roadmap](ROADMAP.md)** - Future development plans

### Quick References
- **[CLI Commands](docs/cli.md)** - Command-line reference
- **[Configuration](docs/config.md)** - Configuration options
- **[Troubleshooting](docs/troubleshooting.md)** - Common issues and solutions

---

## 😎 Contributing

Open to PRs, bugs, ideas, and flames. See [`CONTRIBUTING.md`](CONTRIBUTING.md) for style and module conventions.

---

## 📜 License

MIT License © 2025 [CK Technology LLC](https://github.com/ghostkellz)
See [`LICENSE`](LICENSE) for full terms.

---

## 🩺 System Diagnostics with `reap doctor`

Run `reap doctor` to audit your system, taps, publishers, orphans, and Flatpak status:

```
$ reap doctor
✅ AUR reachable
✅ Tap 'ghostkellz-core' synced 3h ago
⚠️  Tap 'ghostbrew-beta' is stale (last sync: 2 days ago)
✅ All trusted publishers found
⚠️  3 orphaned packages found
⚠️  2 outdated Flatpak packages

Run `reap doctor --fix` to sync, clean, and upgrade.
```

Checks performed:
- AUR reachability
- Tap sync state, index/meta/publisher presence
- GPG trust for publishers
- Orphaned packages
- Flatpak updates (if enabled)

---

## 🔐 Secure-by-Default, Fast Mode, and Fallback Logic

- By default, Reap verifies all tap PKGBUILD signatures if present. If missing, install continues with a warning.
- Use `--strict` or `[security] strict_signatures = true` to require signatures and abort if missing.
- Use `--insecure` or `[security] allow_insecure = true` to bypass all GPG checks (not recommended).
- Use `--fast` or `[perf] fast_mode = true` to skip signature, diff, and dependency checks for speed.
- **No yay/paru fallback: all logic is native Rust.**

☠️ Built with paranoia by **GhostKellz**

---

## 🆕 v0.6.0 New Features

### ⚡ High-Performance Operations
- **Parallel downloads**: Multi-threaded PKGBUILD fetching and search operations
- **Smart caching**: TTL-based cache with automatic warming for popular packages
- **Batch operations**: Install/upgrade multiple packages simultaneously
```bash
reap batch-install firefox discord spotify --parallel
reap parallel-upgrade firefox chromium
reap perf warm-cache                # Preload popular packages
reap perf parallel-search "browser" "editor" "media"
```

### 🛡️ Advanced Security Analysis
- **Enhanced PKGBUILD scanning**: 38 security patterns, risk scoring (0-100)
- **Suspicious domain detection**: URL shorteners, paste sites, temp hosts
- **Credential pattern detection**: Hardcoded passwords, API keys, tokens
- **Security risk scoring**: LOW/MEDIUM/HIGH/CRITICAL classifications
```bash
reap security audit firefox        # Detailed security analysis
reap security scan-all             # Scan all installed packages
reap security stats                # Show security statistics
```

### 🚀 Performance & Caching Commands
- **Cache management**: Statistics, warming, and intelligent cleanup
- **Parallel operations**: Concurrent downloads and processing
- **Performance monitoring**: Operation timing and optimization
```bash
reap perf cache-stats              # Show cache statistics
reap perf parallel-fetch yay firefox discord  # Parallel PKGBUILD fetch
reap perf clear-cache              # Smart cache cleanup
```

### 🔧 Enhanced Batch Operations
- **Multi-package installs**: Handle dependencies and conflicts intelligently
- **Priority-based processing**: Critical packages first, with smart ordering
- **Progress tracking**: Real-time status for batch operations
```bash
reap batch-install pkg1 pkg2 pkg3  # Sequential with backups
reap batch-install pkg1 pkg2 --parallel  # Parallel processing
```

### 📊 System Integration
- **Intelligent backup system**: Pre-install state snapshots with rollback
- **Advanced conflict detection**: File ownership and dependency analysis
- **Performance analytics**: Build time tracking and optimization hints

### 🛡️ Trust & Security Engine
- **Real-time trust scoring**: Every package gets a security score (0-10) with trust badges
- **Security analysis**: PKGBUILD scanning, signature verification, publisher checks
- **Trust badges**: 🛡️ TRUSTED, ✅ VERIFIED, ⚠️ CAUTION, ❌ UNSAFE
```bash
reap trust score firefox    # Show trust analysis
reap trust scan             # Scan all installed packages
```

### ⭐ Community Rating System
- **AUR integration**: Real community votes and popularity scores
- **Star ratings**: Rate packages 1-5 stars with comments
- **Visual display**: ⭐⭐⭐⭐⭐ ratings in TUI and CLI
```bash
reap rate firefox 5 "Excellent browser!"
reap rate vim 4
```

### 🔧 Advanced AUR Operations
- **PKGBUILD fetching**: Manual retrieval and parsing
- **Interactive editing**: Safe PKGBUILD modification with confirmations
- **Conflict detection**: Advanced dependency analysis and circular detection
```bash
reap aur fetch firefox          # Get and analyze PKGBUILD
reap aur edit firefox           # Interactive editing
reap aur deps firefox --conflicts # Check for conflicts
```

### 👤 Multi-Profile Management
- **Profile switching**: Developer, gaming, minimal presets
- **Custom settings**: Backend order, security levels, parallel jobs
```bash
reap profile create dev --template developer
reap profile switch gaming
reap profile list
```

### 📋 Enhanced Interactive TUI
- **5 comprehensive tabs**: Search, Queue, Log, Profiles, System
- **Live monitoring**: Real-time build progress and system stats
- **Trust + rating display**: Combined security and community scores
- **Package details panel**: Full package information with reviews
- **Interactive hotkeys**: `t` trust, `r` rate, `d` diff, `p` profile
