# Configuration

Reaper uses a layered configuration system with clear precedence rules.

## Configuration Files

- Main config: `~/.config/reap/reap.toml`
- Profiles: `~/.config/reap/profiles/`
- Pinned packages: `~/.config/reap/pinned.toml`
- Hooks: `~/.config/reap/hooks/`

## Precedence Order

Configuration is resolved in this order (later overrides earlier):

1. **Built-in defaults**
2. **Config file** (`~/.config/reap/reap.toml`)
3. **Active profile** overrides
4. **Environment variables** (`REAP_*`)
5. **CLI flags** (highest priority)

## Main Configuration

Example `~/.config/reap/reap.toml`:

```toml
# Number of parallel build jobs
parallel = 4

# Backend priority order
backend_order = ["tap", "aur", "pacman", "flatpak"]

# Automatically resolve dependencies
auto_resolve_deps = true

# Skip confirmation prompts
noconfirm = false

[security]
# Require GPG signatures for tap packages
verify_signatures = true

# Require fully trusted signatures where verification is available
strict_mode = false

# Scan PKGBUILDs for suspicious patterns
scan_pkgbuilds = true

# Advisory trust score threshold (0.0 - 10.0)
trust_threshold = 7.0

# How long to cache computed trust scores (hours)
trust_cache_ttl_hours = 24

[build]
# Build in a clean chroot
use_chroot = false

# Clean build directory after install
clean_after_build = true

[devel]
# Check for -git package updates
auto_check = true

# VCS update check interval (hours)
check_interval_hours = 24
```

## Environment Variables

Override config with environment variables:

```bash
export REAP_PARALLEL=8
export REAP_NOCONFIRM=true
export REAP_BACKEND_ORDER="aur,pacman"
```

## CLI Flags

CLI flags always take precedence:

```bash
reap config set parallel 8
reap install package --strict    # Require fully trusted signature
reap install package --insecure  # Skip GPG checks
reap install package --fast      # Skip optional preflight checks
```

## Config Commands

Manage configuration from the CLI:

```bash
# Show current configuration
reap config show

# Get specific value
reap config get parallel

# Set a value
reap config set parallel 8
```

## See Also

- [Profiles](./profiles.md) - Per-profile configuration
- [Commands](./commands.md) - CLI reference
