# Rollback Guide

Reaper provides transaction-based rollback for recovering from problematic package operations.

## How Rollback Works

Reaper records each install, upgrade, and remove operation as a **transaction**. Each transaction captures:

- Which packages were affected
- Previous and new versions
- Paths to cached package artifacts
- Operation type (install, upgrade, remove, etc.)

Rollback works by using cached package artifacts to restore the previous state:

- **Upgrades** are rolled back by reinstalling the previous version from cache (`pacman -U`)
- **Fresh installs** are rolled back by removing the package
- **Removals** are rolled back by reinstalling from the cached artifact

## Important Limitations

**Rollback is artifact-backed, not atomic.** This means:

1. Rollback depends on having the original package artifacts in cache
2. If the pacman cache has been cleaned, older versions may not be available
3. Each package operation during rollback is sequential, not a single atomic transaction
4. System state changes outside of package management (config files, user data) are not restored

**When rollback may not be possible:**

- Pacman cache was cleaned and old package versions are gone
- AUR packages were built but artifacts weren't retained
- Package was installed from a source that no longer has the old version

## Commands

### List Transactions

```bash
# Show last 20 transactions
reap rollback list

# Show last 50 transactions
reap rollback list --limit 50

# Filter by package name
reap rollback list --package firefox
```

Output shows transaction ID, operation type, status, package count, and rollback eligibility.

### Show Transaction Details

```bash
reap rollback show tx_20260406_123456_1234
```

Displays full details including:
- All affected packages with version changes
- Artifact paths and availability
- Rollback eligibility status

### Preview Rollback (Dry Run)

```bash
reap rollback dry-run tx_20260406_123456_1234
```

Shows what would happen without making changes:
- Packages to downgrade (with versions)
- Packages to reinstall
- Packages to remove
- Packages that cannot be rolled back (missing artifacts)
- Dependency warnings and conflicts

### Execute Rollback

```bash
# Interactive (prompts for confirmation)
reap rollback apply tx_20260406_123456_1234

# Auto-confirm
reap rollback apply tx_20260406_123456_1234 -y
```

The apply command:
1. Shows a preview of planned actions
2. Displays any dependency warnings or conflicts
3. Prompts for confirmation (unless `-y` is used)
4. Executes the rollback operations
5. Verifies the results

## Rollback Eligibility

Transactions are categorized by rollback eligibility:

| Status | Meaning |
|--------|---------|
| **Rollbackable** | All packages can be restored |
| **Partially Rollbackable** | Some packages can be restored, others cannot |
| **Not Rollbackable** | No packages can be restored (artifacts missing) |
| **Rolled Back** | Transaction was already rolled back |

## Dependency Analysis

Before rollback, Reaper analyzes potential issues:

### Blockers (shown in red)
- **Dependency Break**: Downgrading would violate a version constraint
- **Version Conflict**: Package conflicts with something installed

### Advisories (shown in yellow)
- **Affected Dependent**: Another package depends on the rollback target
- **Orphaned Dependencies**: Removal may leave unused dependencies
- **Mixed Source**: Transaction includes both repo and AUR packages
- **Provider Change**: Package provides something that changed

## AUR Artifact Retention

For AUR packages, Reaper retains built artifacts in `~/.local/share/reap/artifacts/aur/` to enable rollback. This happens automatically after successful AUR installs.

## Storage Locations

- **Transaction journal**: `~/.local/share/reap/history/transactions/`
- **Retained AUR artifacts**: `~/.local/share/reap/artifacts/aur/`
- **Pacman cache**: `/var/cache/pacman/pkg/` (managed by pacman)

## Example Workflow

```bash
# 1. Install a package
reap install some-package

# 2. Realize there's a problem
# ...

# 3. Find the transaction
reap rollback list
# Output: tx_20260406_143022_5678  Install  Completed  1 pkg  Rollbackable

# 4. Preview what rollback would do
reap rollback dry-run tx_20260406_143022_5678

# 5. Execute the rollback
reap rollback apply tx_20260406_143022_5678
```

## Troubleshooting

### "Artifact missing" error
The package file needed for rollback is no longer in cache. Options:
- Check if the package is available in the Arch Linux Archive
- Manually download the old version and install with `pacman -U`

### "Dependency break" warning
Downgrading would break another package's dependency. Options:
- Also downgrade the dependent package
- Proceed anyway if you understand the risk
- Find an alternative solution

### No transactions listed
Transactions are only recorded for operations performed through Reaper. Operations done directly with pacman/makepkg won't appear.

---

See also:
- [Commands Reference](./commands.md)
- [Configuration](./configuration.md)
