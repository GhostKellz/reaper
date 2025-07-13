## 📦 Unified Package Management Commands

### Global Options
- `--insecure`: Skip GPG verification for tap installs (not recommended)
- `--strict`: Require GPG signature for tap packages; abort if missing
- `--fast`: Fast mode (skip signature, diff, dep tree checks)
- `--gpg-keyserver <url>`: Set keyserver for GPG key auto-fetch

### GPG
- `reap gpg import <keyid>`: Import GPG key
- `reap gpg show <keyid>`: Show GPG key info
- `reap gpg check <keyid>`: Check GPG key
- `reap gpg verify <pkgdir>`: Verify PKGBUILD signature
- `reap gpg set-keyserver <url>`: Set GPG keyserver
- `reap gpg check-keyserver <url>`: Check GPG keyserver

### Core
- `reap install <pkg>` / `-S <pkg>`: Install package (AUR, Flatpak, or tap)
- `reap remove <pkg>` / `-R <pkg>`: Remove package
- `reap update`: Check for package updates (no installation)
- `reap upgradeall` / `-Syu`: Refresh database and upgrade all packages
- `-Sy`: Refresh package database only
- `-Sy <pkg>`: Refresh database and install package
- `-Su`: Upgrade all packages without refreshing database
- `reap local <file>` / `-U <file>`: Install local package
- `reap search <term>` / `-Q <term>`: Search for packages
- `reap pin <pkg>`: Pin package
- `reap clean`: Clean cache
- `reap doctor [--fix]`: System audit (AUR, tap, GPG, orphans, Flatpak); --fix auto-syncs, cleans, upgrades
- `reap tui`: Interactive TUI
- `reap backup`: Backup config

### Flatpak
- `reap flatpak search <query>`: Search Flatpak
- `reap flatpak install <pkg>`: Install Flatpak
- `reap flatpak upgrade`: Upgrade Flatpak
- `reap flatpak audit <pkg>`: Audit Flatpak

### Tap
- `reap tap add <name> <url>`: Add tap repo
- `reap tap list`: List tap repos

### Hooks
- Place executable shell scripts in `~/.config/reap/hooks/` (e.g., `pre_install.sh`, `post_install.sh`)
- Scripts receive context via env vars: `REAP_PKG`, `REAP_VERSION`, etc.
- Lua scripting: planned as a future advanced feature

### Misc
- `reap completion <shell>`: Shell completion

### Examples

- `reap update`                 # Check for package updates
- `reap -Syu`                   # Update database and upgrade all packages (like other AUR helpers)
- `reap -Sy htop`               # Update database and install htop
- `reap -S htop`                # Install htop
- `reap -R htop`                # Remove htop
- `reap install htop --strict`  # Require GPG signature
- `reap install foo --fast`     # Fast mode (skip signature, diff, dep tree)
- `reap install bar --insecure` # Skip all GPG checks
- `reap rollback htop`          # Rollback package
- `reap tap add mytap https://github.com/me/mytap.git` # Add a tap
- `reap doctor --fix`           # Auto-fix system issues

---

See [DOCS.md](./DOCS.md) for full documentation and usage details.

