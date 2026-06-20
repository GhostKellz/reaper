<p align="center">
  <img src="assets/reaper-logo.png" alt="Reaper Logo" width="200">
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Arch_Linux-1793D1?style=for-the-badge&logo=arch-linux&logoColor=white" alt="Arch Linux">
  <img src="https://img.shields.io/badge/Rust-B7410E?style=for-the-badge&logo=rust&logoColor=white" alt="Rust">
  <img src="https://img.shields.io/badge/Clap-E64A19?style=for-the-badge&logo=rust&logoColor=white" alt="Clap">
  <img src="https://img.shields.io/badge/Tokio-463370?style=for-the-badge&logo=rust&logoColor=white" alt="Tokio">
  <img src="https://img.shields.io/badge/Ratatui-00CED1?style=for-the-badge&logo=gnome-terminal&logoColor=white" alt="Ratatui">
  <img src="https://img.shields.io/badge/License-MIT-2ea44f?style=for-the-badge" alt="MIT License">
</p>

# ☠️ Reaper Package Manager

---

## 📄 Overview

**Reaper** is a blazing-fast, Rust-powered **AUR helper and meta package manager** for Arch Linux. It is designed for paranoid Arch users, power packagers, and automation-first workflows.

---

## 🔧 Capabilities

* Unified search: AUR, Pacman, Flatpak, ChaoticAUR, ghostctl-aur, custom binary repos
* Interactive TUI installer with multi-source search and install queue
* GPG key importing, PKGBUILD diff and auditing, trust level reporting
* Supply-chain auditing: scans PKGBUILD **and** `.install` hooks for build-time and supply-chain attack patterns
* PKGBUILD review prompts with persisted accepted baselines and changed-line previews
* Transaction-based rollback with artifact-backed downgrades and multi-package upgrades
* Flatpak integration with metadata and audit
* Orphan detection and removal (AUR/pacman)
* Install and upgrade planning with dependency, make dependency, conflict, and source preview
* Binary-only and repo selection (e.g. --repo=ghostctl-aur)
* Shell-based pre/post-install hooks for automation
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
reap rollback list                  # List recent transactions (roll back by txid)
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
- See [Commands Reference](docs/usage/commands.md) for the full updated command list.

### Tap GPG Verification Example

```bash
reap install <pkg> --strict
# Aborts if PKGBUILD.sig is missing or invalid
reap install <pkg> --insecure
# Skips all GPG checks
```

### Rollback Example

```bash
reap rollback list                  # Find the transaction id (txid) to revert
reap rollback dry-run <txid>        # Preview the downgrades/removals
reap rollback apply <txid>          # Execute the rollback
```

---

## 🚀 Enhanced Usage Examples

```bash
# 🔍 Enhanced package operations with trust and ratings
reap install firefox --diff          # Show PKGBUILD diff before install
reap install firefox                 # Preview install plan, then build/install
reap install firefox --dry-run       # Preview the plan and PKGBUILD review without side effects
reap install firefox --skipreview    # Skip structured PKGBUILD review prompts
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

# 📋 Interactive TUI
reap tui                              # Launch the TUI
# Hotkeys: t=trust, d=diff, p=profiles tab, TAB=cycle tabs

# 🛡️ Security and trust operations
reap trust scan                       # Scan all packages for security
reap trust stats                      # Show trust statistics
reap security audit firefox           # Audit PKGBUILD + .install for supply-chain patterns
reap security scan-all                # Audit every installed AUR package

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
  2. If the key is missing, Reaper will auto-fetch it from the default keyserver (`hkps://keys.openpgp.org`).
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
- Generate a GPG key (see [Tap Publishing](docs/usage/tap-publishing.md)).
- Sign your PKGBUILD: `gpg --detach-sign --armor PKGBUILD`
- Add your info to publisher.toml and commit both files to your tap repo.

**For users:**
- Use `reap install <pkg>` as normal. Reaper will verify the signature and show publisher info.
- Use `--insecure` to skip verification (not recommended).

See [GPG Verification](docs/security/gpg-verification.md) for details.

---

## ⚠️ Install Conflict Handling

During dependency resolution Reap inspects package file ownership (`pacman -Ql` / `pacman -Qo`) and reports packages whose files are already owned by another package. Pacman itself enforces file conflicts at install time, so a genuine conflict aborts the `pacman -U` step.

---

## 🔙 Rollback System

Every install/upgrade/remove is recorded in a transaction journal under `~/.local/share/reap/history/transactions/`. Rollback replays a transaction using cached package artifacts (the pacman cache and retained AUR builds), downgrading or reinstalling the previous versions.

```bash
reap rollback list                  # Find the transaction id (txid)
reap rollback show <txid>           # Inspect a transaction
reap rollback dry-run <txid>        # Preview the planned actions
reap rollback apply <txid>          # Execute the rollback
```

---

## 🔍 Smart Search with Tap Priorities

Reap searches all enabled taps first, respecting per-tap priority.

Set a tap's priority when adding it, e.g. `reap tap add ghost <url> --priority 10`.
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

Reap resolves and installs dependencies from taps, AUR, or your system — no yay/paru needed. PKGBUILD and tap metadata dependencies are satisfied automatically (controlled by `auto_resolve_deps` in `reap.toml`).

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

---

## 📂 Config Example

See [Configuration Guide](docs/usage/configuration.md) for advanced configuration.

---

## 📚 Documentation

**[Full Documentation](docs/README.md)** - Complete documentation index

### Getting Started
- **[Quick Start](docs/getting-started/quickstart.md)** - Get up and running
- **[Installation](docs/getting-started/installation.md)** - Build and install

### User Documentation
- **[Commands](docs/usage/commands.md)** - CLI command reference
- **[Configuration](docs/usage/configuration.md)** - Configuration options
- **[Profiles](docs/usage/profiles.md)** - Multi-profile management
- **[Tap Publishing](docs/usage/tap-publishing.md)** - Publishing tap packages

### Security
- **[Security Guide](SECURITY.md)** - Security overview
- **[Trust Model](docs/security/trust-model.md)** - Advisory trust system
- **[GPG Verification](docs/security/gpg-verification.md)** - Signature verification

### Developer Documentation
- **[API Reference](docs/development/api.md)** - API documentation
- **[Architecture](docs/development/architecture.md)** - System design
- **[Development Guide](docs/development/dev-guide.md)** - Development tips
- **[Contributing](CONTRIBUTING.md)** - Contribution guide

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
```

Checks performed:
- AUR reachability
- Tap sync state, index/meta/publisher presence
- GPG trust for publishers
- Orphaned packages
- Flatpak updates (if enabled)

---

## 🔐 Secure-by-Default, Fast Mode, and Fallback Logic

- By default, Reap requires valid tap PKGBUILD signatures when installing tap packages.
- Use `--strict` or `[security] strict_mode = true` to require fully trusted signatures where verification is available.
- Use `--insecure` to bypass GPG checks for a single install (not recommended).
- Use `--fast` to skip optional preflight checks for speed.
- **No yay/paru fallback: all logic is native Rust.**

☠️ Built with paranoia by **GhostKellz**

---

## Key Features

### Performance
- **Parallel operations**: Multi-threaded PKGBUILD fetching and search
- **Smart caching**: TTL-based cache for AUR queries
- **Native Rust**: No yay/paru fallback, all logic is async Rust

### Security
- **Trust scoring**: Advisory-only security scores (0-10)
- **GPG verification**: Signature verification for tap packages
- **PKGBUILD analysis**: Pattern detection for suspicious content

### AUR Operations
- **Dependency resolution**: Handles providers, conflicts, circular deps
- **PKGBUILD management**: Fetch, view diff, edit before install
- **Devel packages**: Automatic -git package update detection

### Profiles
- **Multi-profile**: Switch between dev/gaming/minimal configurations
- **Custom settings**: Backend order, parallel jobs, security levels

### TUI
- **Interactive interface**: Search, queue, logs, profiles, system tabs
- **Trust integration**: View trust scores and PKGBUILD diffs inline

See [CHANGELOG.md](CHANGELOG.md) for version history.
