# 📘 Reaper Documentation

## Overview

Reaper (`reap`) is a modern, async-first AUR helper and unified package manager written in Rust. It supports AUR, Flatpak, and tap-based sources, with a focus on security, extensibility, and a clean CLI/TUI experience.

## Installation

See the README for installation instructions.

## CLI Usage

### Core Commands

- `reap -S <pkg>` / `reap install <pkg>`: Install AUR or repo package (supports --parallel, --edit)
- `reap -R <pkg>` / `reap remove <pkg>`: Remove a package
- `reap -Syu` / `reap upgradeall`: Sync and upgrade all packages
- `reap -U <file>` / `reap local <file>`: Install local package file
- `reap -Q <term>` / `reap search <term>`: Search AUR (and Flatpak)
- `reap pin <pkg>`: Pin a package to exclude from upgrades
- `reap clean`: Clean cache and temp files
- `reap doctor`: Run system health check and config audit
- `reap tui`: Launch the interactive TUI
- `reap backup`: Backup current config to backup directory

### Flatpak Commands

- `reap flatpak search <query>`: Search Flatpak packages
- `reap flatpak install <pkg>`: Install a Flatpak package
- `reap flatpak upgrade`: Upgrade Flatpak packages
- `reap flatpak audit <pkg>`: Audit Flatpak sandbox info

### GPG Commands

- `reap gpg import <keyid>`: Import a GPG key
- `reap gpg show <keyid>`: Show GPG key info and trust level
- `reap gpg check <keyid>`: Check GPG key
- `reap gpg verify <pkgdir>`: Verify PKGBUILD signature in a directory
- `reap gpg set-keyserver <url>`: Set the GPG keyserver
- `reap gpg check-keyserver <url>`: Check if a GPG keyserver is reachable

### Tap Commands

- `reap tap add <name> <url>`: Add a tap repository
- `reap tap list`: List configured tap repositories

### Other

- `reap completion <shell>`: Generate shell completion for bash, zsh, or fish

## Configuration

- Config files are stored in `~/.config/reap/`
- Backups are stored in `/var/lib/reaper/backups/`
- Pinning: `~/.config/reap/pinned.toml`
- Tap repos: `~/.config/reap/taps.json`
- Config precedence: CLI flag > `~/.config/reap/reap.toml` > default
- Config is validated on load; errors will abort with a clear message.

## Hooks and Automation

Reap supports shell-based hooks for automation and plugin-style behavior. You can define hooks as executable `.sh` scripts in:
- `~/.config/reap/hooks/pre_install.sh`
- `~/.config/reap/hooks/post_install.sh`
- `~/.config/reap/hooks/post_upgrade.sh`
- etc.

Each script receives context via environment variables:
- `REAP_PKG`, `REAP_VERSION`, `REAP_SOURCE`, `REAP_INSTALL_PATH`, `REAP_TAP`

Example:
```sh
#!/bin/bash
echo "[HOOK] Installing $REAP_PKG from $REAP_SOURCE"
```

> Advanced: Lua scripting support is planned as a future, low-priority feature for advanced users.

### Supported Hooks
- `pre_install(pkg)`
- `post_install(pkg)`
- `post_upgrade(pkg)`
- `on_conflict(pkg)`
- `on_flatpak_search(query)`
- `on_flatpak_install(pkg)`

Hooks are triggered automatically at key points. Example:

```lua
-- ~/.config/reap/hooks/post_install.lua
function post_install(pkg)
  print("✅ Custom post-install hook ran for " .. pkg)
end
```

You can use hooks to:
- Send Discord/webhook notifications
- Filter or tag Flatpak apps
- Enforce custom policies

Reap will look for per-tap hooks first, then global hooks.

---

## Security

- GPG verification is integrated for PKGBUILD signatures
- Keyserver fallback and trust level reporting

## TUI

- Run `reap tui` for an interactive terminal UI (early stage)

## System Diagnostics

Run `reap doctor` to check:
- AUR connectivity
- Tap sync and integrity (index.json, meta.toml, publisher.toml)
- GPG trust for publishers
- Orphaned packages (with --remove to clean)
- Flatpak updates (if backend enabled)

Use `reap doctor --fix` to auto-fix stale taps, remove orphans, and upgrade Flatpaks.

## Publisher Verification and GPG

Taps must include a `publisher.toml` file with:

```toml
name = "GhostKellz"
email = "ckelley@ghostkellz.sh"
gpg_key = "F7C2 0EFD 6F3E 9A88 F14A  77F3 CDEE 9E44 E881 E42E"
verified = true
url = "https://ghostkellz.sh"
```

### How Verification Works
- On install, Reap checks for `PKGBUILD.sig` and verifies it with GPG.
- The publisher's GPG key (from `publisher.toml`) must be in your keyring. If missing, Reap will auto-fetch it (or prompt).
- If verification fails, install is aborted unless `--insecure` is passed.
- Publisher info and verification status are shown in the output and logs.

#### Example Output

```
🔐 Publisher: GhostKellz <ckelley@ghostkellz.sh>
🔑 GPG Key: E881E42E [Verified]
📦 Tap: ghostkellz-core
✅ PKGBUILD signature verified
```

If not verified:

```
❌ Verification failed for PKGBUILD.sig
✋ Aborting install. Use --insecure to override.
```

#### CLI Options
- `--insecure` – skip GPG verification (not recommended)
- `--gpg-keyserver <url>` – set keyserver for auto-fetch

#### Tap Integrity (Optional)
If the tap provides a signed `index.json` or `tap.sig`, Reap will verify the full tap for extra security.

## Secure Tap Installs & Publisher Verification

Reaper enforces GPG signature verification for all tap-based packages by default.

### How it works
- Each tap must provide a `publisher.toml` with publisher info and GPG key.
- Each package must include a `PKGBUILD.sig` (GPG signature of PKGBUILD).
- On install, Reaper:
  1. Checks for `PKGBUILD.sig` and verifies it with GPG.
  2. Ensures the publisher's GPG key is in your keyring (auto-fetches if missing).
  3. Aborts install if verification fails (unless `--insecure` is passed).
  4. Shows publisher info and verification status in output/logs.

### CLI Options
- `--insecure` – Skip GPG verification for tap installs (not recommended)
- `--gpg-keyserver <url>` – Set keyserver for GPG key auto-fetch (default: hkps://keys.openpgp.org)

### Key Fetching
If the publisher's GPG key is not present, Reaper will attempt to fetch it from the specified keyserver. You can override the keyserver with `--gpg-keyserver` or set it in your config.

### Example Usage
```bash
reap install ghostctl --backend=tap --resolve-deps
reap install ghostctl --insecure
```

### For Publishers
See [PUBLISHING.md](PUBLISHING.md) for a full walkthrough on signing and publishing tap packages.

## Install Decision Matrix

| PKGBUILD.sig Present | --strict or strict_signatures | --insecure or allow_insecure | --fast or fast_mode | Behavior |
|---------------------|------------------------------|------------------------------|---------------------|----------|
| Yes                 | Any                          | false                        | false               | Verify signature, abort on failure |
| No                  | true                         | false                        | false               | Abort install |
| No                  | false                        | false                        | false               | Warn, continue |
| Any                 | Any                          | true                         | Any                 | Skip all GPG checks |
| Any                 | Any                          | Any                          | true                | Skip signature, diff, dep tree |

### How to Opt In/Out
- Use `--strict` to require signatures.
- Use `--insecure` to skip all GPG checks.
- Use `--fast` to skip signature, diff, and dep tree for speed.
- Or set these in `[security]` and `[perf]` in `reap.toml`.

## See also

- [ROADMAP.md](./ROADMAP.md) for planned features and status
- [COMMANDS.md](./COMMANDS.md) for a concise command reference

