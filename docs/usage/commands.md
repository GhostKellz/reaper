# Command Reference

## Core Commands

### Package Management
- `reap install <pkg>` / `-S <pkg>`: Install a package from AUR, repo, or tap
  - `--repo <name>`: Install from specific repository
  - `--binary-only`: Prefer binary packages
  - `--diff`: Show PKGBUILD diff before install
  - `--insecure`: Skip signature verification (use with caution)
- `reap remove <pkg>` / `-R <pkg>`: Remove packages
- `reap search <term>` / `-Ss <term>`: Search for packages
- `reap update`: Check for available updates
- `reap upgrade`: Upgrade all packages
- `reap upgrade-all` / `-Syu`: Refresh database and upgrade all
- `reap batch-install <pkgs...>`: Install multiple packages
  - `--parallel`: Install in parallel
- `reap parallel-upgrade <pkgs...>`: Upgrade specific packages in parallel
- `reap local <files...>` / `-U <file>`: Install local package files
- `reap pin <pkg>`: Pin a package to current version

### Pacman-Style Shortcuts
- `-S <pkg>`: Install package
- `-Ss <term>`: Search packages
- `-R <pkg>`: Remove package
- `-Syu`: Sync database and upgrade all
- `-Sy`: Refresh package database only
- `-Su`: Upgrade without refreshing database
- `-Qu`: Query upgradable packages
- `-Q <pkg>`: Query if package is installed
- `-Sc`: Clean package cache

## Transaction & Rollback

- `reap rollback list [--limit N] [--package PKG]`: List recent transactions
- `reap rollback show <txid>`: Show transaction details
- `reap rollback dry-run <txid>`: Preview rollback actions
- `reap rollback apply <txid> [-y]`: Execute rollback

See [Rollback Guide](./rollback.md) for detailed usage.

## Tap Repositories

- `reap tap add <name> <url>`: Add a tap repository
- `reap tap remove <name>`: Remove a tap repository
- `reap tap list`: List configured taps
- `reap tap sync [name]`: Sync tap repositories

## Flatpak

- `reap flatpak search <query>`: Search Flatpak apps
- `reap flatpak install <app>`: Install Flatpak app
- `reap flatpak remove <app>`: Remove Flatpak app
- `reap flatpak list`: List installed Flatpak apps
- `reap flatpak upgrade`: Upgrade all Flatpak apps
- `reap flatpak audit <app>`: Audit Flatpak permissions
- `reap flatpak-upgrade`: Shortcut for `flatpak upgrade`

## Security & Trust

### Trust Commands
- `reap trust score <pkg>`: Analyze trust score for a package
- `reap trust scan`: Scan all installed AUR packages for trust scores
- `reap trust stats`: Show aggregate trust statistics
- `reap trust update`: Update trust database

### Security Commands
- `reap security audit <pkg>`: Audit a package's PKGBUILD and `.install` hook for risky and supply-chain patterns
- `reap security scan-all`: Audit every installed AUR package
- `reap security stats`: Show security statistics
- `reap security update-rules`: Update security rules

The audit scans both the PKGBUILD and the `.install` hook (when present), flagging build-time code-execution and supply-chain techniques such as bundled hook execution, npm/bun dependency installs, npm lifecycle hooks, and Tor C2 endpoints.

### GPG Commands
- `reap gpg import <keyid>`: Import a GPG key
- `reap gpg show <keyid>`: Show GPG key info
- `reap gpg check <keyid>`: Check if key is available
- `reap gpg verify <path>`: Verify PKGBUILD signature
- `reap gpg set-keyserver <url>`: Set GPG keyserver
- `reap gpg check-keyserver <url>`: Test keyserver connectivity

## System Maintenance

- `reap doctor`: Run system diagnostics
- `reap clean`: Clean package cache
- `reap orphan`: List orphaned packages
  - `--remove`: Remove orphaned packages
  - `--all`: Include pacman orphans (not just AUR)
- `reap sync-db`: Sync package database

## Configuration

- `reap config show`: Show current configuration
- `reap config get <key>`: Get a configuration value
- `reap config set <key> <value>`: Set a configuration value
- `reap backup`: Backup configuration

## Profiles

- `reap profile list`: List available profiles
- `reap profile show [name]`: Show profile details
- `reap profile switch <name>`: Switch to a profile
- `reap profile create <name>`: Create a new profile
- `reap profile delete <name>`: Delete a profile

## Other

- `reap tui`: Launch interactive TUI
- `reap audit <pkg>`: Audit a specific package
- `reap rate <pkg> -r <1-5> [-c "comment"]`: Rate a package
- `reap completion <shell>`: Generate shell completions

## Examples

```bash
# Basic package management
reap install htop              # Install htop
reap -S htop                   # Same as above (pacman style)
reap remove htop               # Remove htop
reap search vim                # Search for vim packages

# System updates
reap -Syu                      # Full system upgrade
reap update                    # Check for updates (no install)
reap upgrade                   # Upgrade all packages

# Rollback operations
reap rollback list             # List recent transactions
reap rollback show tx_20260406_123456_1234
reap rollback dry-run tx_20260406_123456_1234
reap rollback apply tx_20260406_123456_1234

# Tap repositories
reap tap add mytap https://github.com/me/mytap.git
reap tap list
reap tap sync

# System maintenance
reap doctor                    # System diagnostics
reap orphan --remove           # Remove orphan packages
reap clean                     # Clean package cache
```

---

See also:
- [Configuration](./configuration.md)
- [Profiles](./profiles.md)
- [Rollback Guide](./rollback.md)
- [Trust Model](../security/trust-model.md)
