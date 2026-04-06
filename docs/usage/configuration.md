# Configuration

Reaper uses a layered configuration system with clear precedence rules.

## Configuration Files

- Main config: `~/.config/reaper/reap.toml`
- Profiles: `~/.config/reaper/profiles/`
- Pinned packages: `~/.config/reaper/pinned.toml`
- Hooks: `~/.config/reaper/hooks/`

## Precedence Order

Configuration is resolved in this order (later overrides earlier):

1. **Built-in defaults**
2. **Config file** (`~/.config/reaper/reap.toml`)
3. **Active profile** overrides
4. **Environment variables** (`REAP_*`)
5. **CLI flags** (highest priority)

## Main Configuration

Example `~/.config/reaper/reap.toml`:

```toml
# Number of parallel build jobs
parallel = 4

# Backend priority order
backend_order = ["aur", "pacman", "flatpak"]

# Automatically resolve dependencies
auto_resolve_deps = true

# Skip confirmation prompts
noconfirm = false

[security]
# Require GPG signatures for tap packages
strict_signatures = false

# Allow insecure installs (not recommended)
allow_insecure = false

[build]
# Clean build directory after install
clean_after_build = true

# Build directory location
build_dir = "~/.cache/reaper/build"

[devel]
# Check for -git package updates
check_devel = true

# VCS update check interval (hours)
check_interval = 12
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
reap install firefox --parallel 8
reap install package --strict    # Require GPG signature
reap install package --insecure  # Skip GPG checks
reap install package --fast      # Skip verification steps
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
