# Profiles

Profiles let you switch between different package management configurations for different workflows.

## Profile Basics

```bash
# List all profiles
reap profile list

# Show current profile
reap profile show

# Switch profiles
reap profile switch dev
```

## Creating Profiles

Create a new profile from a template:

```bash
reap profile create myprofile --template developer
```

Available templates:
- `developer` - Strict security, tap-first backend order
- `gaming` - Relaxed security, flatpak-first for game launchers
- `minimal` - Conservative settings, pacman/AUR only

## Profile Configuration

Profiles can override these settings:

| Setting | Description |
|---------|-------------|
| `backend_order` | Priority order for package sources |
| `parallel_jobs` | Number of concurrent build jobs |
| `strict_signatures` | Require GPG signatures |
| `fast_mode` | Skip verification for speed |
| `auto_resolve_deps` | Automatically resolve dependencies |

## Profile Storage

Profiles are stored in `~/.config/reaper/profiles/`:

```
~/.config/reaper/profiles/
├── .active           # Contains name of active profile
├── dev.toml
├── gaming.toml
└── minimal.toml
```

## Example Profile

`~/.config/reaper/profiles/dev.toml`:

```toml
name = "dev"
backend_order = ["tap", "aur", "pacman"]
parallel_jobs = 8
strict_signatures = true
auto_resolve_deps = true
```

## Profile-Aware Operations

All package operations respect the active profile:

```bash
# With 'dev' profile active (strict mode)
reap install package
# Uses tap-first backend order, requires signatures

# Switch to gaming profile
reap profile switch gaming
reap install steam
# Uses flatpak-first, relaxed security
```

## See Also

- [Configuration](./configuration.md) - Base configuration
- [Commands](./commands.md) - CLI reference
