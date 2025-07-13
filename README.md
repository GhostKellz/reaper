# ☠️ Reaper Package Manager

[![Platform: Arch Linux](https://img.shields.io/badge/platform-Arch%20Linux-1793d1?logo=arch-linux\&logoColor=white)](https://archlinux.org)
[![Made with Zig](https://img.shields.io/badge/made%20with-Zig-f7a41d?logo=zig\&logoColor=white)](https://ziglang.org/)
[![Zig Dev](https://img.shields.io/badge/zig-dev%200.15-f7a41d?logo=zig\&logoColor=white)](https://ziglang.org/download/)
[![ZSync Runtime](https://img.shields.io/badge/runtime-zsync%20async-blue?logo=zig\&logoColor=white)](https://github.com/ghostkellz/zsync)
[![Build Status](https://img.shields.io/github/actions/workflow/status/GhostKellz/reaper/main.yml?branch=main)](https://github.com/GhostKellz/reaper/actions)
![License](https://img.shields.io/github/license/GhostKellz/reaper)

---

## 📄 Overview

**Reaper** is a blazing-fast, Zig-powered **AUR helper and meta package manager** for Arch Linux.
Built with Zig 0.15-dev and the [zsync async runtime](https://github.com/ghostkellz/zsync), it provides unparalleled performance and memory safety.
*This is the modern Zig implementation. For the original Rust version, see [reaper-rs](https://github.com/GhostKellz/reaper-rs).*

Designed for paranoid Arch users, power packagers, and automation-first workflows with native async operations and zero-allocation performance paths.

---

## 🚀 Why Zig?

* **Memory Safety:** Compile-time safety without garbage collection
* **Zero-Cost Abstractions:** C-like performance with modern features
* **Native Async:** Built-in async/await via zsync runtime for optimal I/O
* **Cross-Platform:** Compile for all architectures, out of the box
* **Predictable Performance:** No hidden allocations, no GC surprises

---

## 🔧 Capabilities

* Unified search: AUR, Pacman, Flatpak, ChaoticAUR, ghostctl-aur, custom binary repos
* Interactive TUI installer with multi-source search & install queue
* GPG key importing, PKGBUILD diff/auditing, trust level reporting
* Rollback system, backup/restore, multi-package upgrades
* Flatpak integration with metadata & audit
* Orphan detection/removal (AUR/pacman)
* Dependency & conflict resolution with `--resolve-deps`
* Binary-only and repo selection (e.g. `--repo=ghostctl-aur`)
* Plugin/hook support (native Zig performance)
* Search and PKGBUILD caching for speed
* **No yay/paru fallback** — all install/upgrade logic is native async Zig (zsync)

---

## 📦 Installation

### 🔥 Quick Install

```bash
curl -sSL https://raw.githubusercontent.com/GhostKellz/reaper/main/install.sh | bash
```

### Build from Source

Requirements:

* Zig 0.15.0-dev or later
* Git

```bash
git clone https://github.com/GhostKellz/reaper.git
cd reaper
zig build -Doptimize=ReleaseFast
```

### Install

```bash
zig build install --prefix ~/.local         # Recommended (user)
sudo zig build install --prefix /usr/local  # System-wide
```

### Development Build

```bash
zig build
zig build run -- --help
```

### Planned: AUR Binary

```bash
yay -S reaper-bin
```

---

## 🚀 Usage

```bash
reap install <pkg> --fast              # Fast: skip sig, diff, dep tree
reap install <pkg> --strict            # Require GPG signature, abort if missing
reap install <pkg> --insecure          # Skip GPG checks (not recommended)
reap install <pkg> --repo=ghostctl-aur # Force binary repo
reap upgrade                           # Upgrade all packages
reap rollback <pkg>                    # Rollback a package
reap orphan [--remove]                 # List/remove orphaned packages
reap backup                            # Backup config
reap diff <pkg>                        # Show PKGBUILD diff
reap pin <pkg>                         # Pin a package/version
reap clean                             # Clean cache
reap doctor                            # System/config health check
reap tui                               # Launch interactive TUI
reap gpg ...                           # GPG key management
reap flatpak ...                       # Flatpak management
reap tap ...                           # Tap repo management
```

* All install/upgrade flows are async/parallel using zsync runtime and do not use yay/paru fallback.
* TUI and CLI support all major commands, including rollback, pin, audit, and tap management.
* Native Zig performance with zero-allocation paths for critical operations.
* Built on Zig 0.15 dev with modern async/await patterns via zsync.
* See [MAKE\_COMMANDS.md](MAKE_COMMANDS.md) for full command list.

---

## 🔐 Secure Publisher Verification

Reaper ensures that all tap-based packages are cryptographically verified before install (unless you use `--insecure`).

* **PKGBUILD.sig:** Each tap package must include a GPG signature for its PKGBUILD file.
* **publisher.toml:** Each tap must provide publisher info & GPG fingerprint.
* **Verification flow:**

  1. Reaper checks for `PKGBUILD.sig` and verifies with publisher's GPG key.
  2. Missing key? Auto-fetch from a keyserver (configurable).
  3. If verification fails, install is aborted unless `--insecure` is passed.
  4. Publisher info and verification status are shown in CLI & TUI.

**publisher.toml example:**

```toml
name = "GhostKellz"
gpg_key = "F7C2 0EFD 6F3E 9A88 F14A  77F3 CDEE 9E44 E881 E42E"
email = "ckelley@ghostkellz.sh"
verified = true
url = "https://ghostkellz.sh"
```

For tap publishers: generate a GPG key, sign PKGBUILD, add info to publisher.toml and commit both files to your tap repo.

For users: `reap install <pkg>` verifies signatures and shows publisher info.

---

## ⚠️ Install Conflict Handling

Before installing, Reap checks for file conflicts using `pacman -Qo`. If a conflict is found:

```
⚠️ Conflict: /usr/bin/foo is owned by pacman:foo. Use --force to override.
```

Use `--force` only if you know what you are doing.

---

## 🔙 Rollback & Backup System

Before every install, Reap backs up current package state (pacman db/binaries) to `~/.local/share/reap/backup/<pkg>/<timestamp>/`.
To rollback:

```bash
reap rollback <pkg>
```

---

## 🔍 Smart Search with Tap Priorities

Reap searches all enabled taps first, respecting per-tap priority. Results merge with AUR/Flatpak and are sorted accordingly.
Set tap priority:

```bash
reap tap set-priority ghost 10
```

---

## 🔁 Tap Auto-Sync

Reap automatically syncs all enabled taps before running search/install operations. Configure sync in `~/.config/reap/reap.toml`:

```toml
[settings]
auto_sync = true
sync_interval_hours = 6
```

Manual sync:

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

Config precedence: CLI flag > `~/.config/reap/reap.toml` > default.

---

## 🤖 Hooks and Automation

Reap supports shell-based hooks for automation. Place executable `.sh` scripts in `~/.config/reap/hooks/` (e.g., `pre_install.sh`, `post_install.sh`).
Each script receives context via env vars: `REAP_PKG`, `REAP_VERSION`, `REAP_SOURCE`, `REAP_INSTALL_PATH`, `REAP_TAP`.

Example:

```sh
echo "[HOOK] Installing $REAP_PKG from $REAP_SOURCE"
```

---

## 📚 Documentation

### User Docs

* [Features Guide](FEATURES.md)
* [Security Guide](SECURITY.md)
* [Profile Management](docs/profiles.md)
* [Trust System](docs/trust.md)
* [Interactive Features](docs/interactive.md)
* [TUI Guide](docs/tui.md)

### Developer Docs

* [API Reference](API.md)
* [Architecture](ARCHITECTURE.md)
* [Contributing](CONTRIBUTING.md)
* [Roadmap](ROADMAP.md)

### Quick References

* [CLI Commands](docs/cli.md)
* [Configuration](docs/config.md)
* [Troubleshooting](docs/troubleshooting.md)

---

## 😎 Contributing

Open to PRs, bugs, ideas, and flames. See [`CONTRIBUTING.md`](CONTRIBUTING.md) for style and module conventions.

---

## 📜 License

MIT License © 2025 [CK Technology LLC](https://github.com/ghostkellz)
See [`LICENSE`](LICENSE) for full terms.

---

## 🩺 System Diagnostics: `reap doctor`

Audit your system, taps, publishers, orphans, and Flatpak status:

```bash
reap doctor
```

Sample output:

```
✅ AUR reachable
✅ Tap 'ghostkellz-core' synced 3h ago
⚠️  Tap 'ghostbrew-beta' is stale (last sync: 2 days ago)
✅ All trusted publishers found
⚠️  3 orphaned packages found
⚠️  2 outdated Flatpak packages

Run `reap doctor --fix` to sync, clean, and upgrade.
```

Checks performed:

* AUR reachability
* Tap sync state, index/meta/publisher presence
* GPG trust for publishers
* Orphaned packages
* Flatpak updates (if enabled)

---

## 🆕 v0.6.0 Feature Highlights

* **Parallel downloads**: Multi-threaded PKGBUILD fetching/search
* **Smart caching**: TTL-based cache, automatic warming
* **Batch operations**: Multi-package install/upgrade, progress
* **Advanced security**: PKGBUILD scanning, risk scoring, trust badges
* **Conflict detection & rollback**: File ownership, backup/restore
* **Multi-profile management**: Switch between dev, gaming, minimal
* **Enhanced TUI**: Search, queue, log, profiles, system, live stats

---

☠️ Built with paranoia by **GhostKellz**
