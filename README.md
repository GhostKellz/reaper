[![Arch Linux](https://img.shields.io/badge/platform-Arch%20Linux-1793d1?logo=arch-linux&logoColor=white)](https://archlinux.org)
[![Made with Rust](https://img.shields.io/badge/made%20with-Rust-000000?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Status](https://img.shields.io/badge/status-active-success?style=flat-square)](https://github.com/ghostkellz/reaper)
[![Build](https://img.shields.io/github/actions/workflow/status/ghostkellz/reaper/main.yml?branch=main)](https://github.com/ghostkellz/reaper/actions)
![Built with Clap](https://img.shields.io/badge/built%20with-clap-orange)
![License](https://img.shields.io/github/license/ghostkellz/reaper)

# ☠️ Reaper Package Manager

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

* [Command Reference](COMMANDS.md)
* [Full Docs](DOCS.md)
* `reap doctor` – validate your environment

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

