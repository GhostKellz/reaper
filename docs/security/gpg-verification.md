# GPG Verification

Reaper integrates with GPG for package signature verification.

## How It Works

When installing from a tap repository:

1. Reaper checks for `PKGBUILD.sig` (GPG signature file)
2. If present, verifies the signature using `gpg --verify`
3. Checks that the signing key matches `publisher.toml`
4. Displays verification status in output

## Verification Results

| Result | Behavior |
|--------|----------|
| Valid signature | Shows green verification badge |
| Invalid signature | Shows warning, continues install |
| Missing signature | Shows info message, continues install |
| Missing key | Attempts to fetch from keyserver |

## GPG Commands

```bash
# Import a GPG key
reap gpg import <keyid>

# Show key information
reap gpg show <keyid>

# Check if a key exists
reap gpg check <keyid>

# Verify PKGBUILD signature manually
reap gpg verify /path/to/pkgbuild

# Set keyserver for key fetching
reap gpg set-keyserver hkps://keys.openpgp.org

# Test keyserver connectivity
reap gpg check-keyserver hkps://keys.openpgp.org
```

## Signature Modes

### Default Mode

Verifies signatures when present, warns if missing or invalid, but doesn't block:

```bash
reap install package
```

### Strict Mode

Requires valid signature - aborts if missing or invalid:

```bash
reap install package --strict
```

Or in config:
```toml
[security]
strict_signatures = true
```

### Insecure Mode

Skips all GPG verification (not recommended):

```bash
reap install package --insecure
```

## Key Management

GPG keys are stored in your standard GPG keyring (`~/.gnupg/`).

Import trusted publisher keys:

```bash
# From keyserver
gpg --keyserver hkps://keys.openpgp.org --recv-keys <keyid>

# Or via reaper
reap gpg import <keyid>
```

## For Publishers

See [Tap Publishing](../usage/tap-publishing.md) for how to sign your packages.

## Troubleshooting

**"Key not found" errors:**
```bash
# Manually import the key
gpg --keyserver hkps://keys.openpgp.org --recv-keys <keyid>
```

**"Bad signature" errors:**
- The package may have been modified after signing
- The wrong key may be used for verification
- Check `publisher.toml` matches the actual signing key

**Keyserver unreachable:**
```bash
# Try a different keyserver
reap gpg set-keyserver hkps://keyserver.ubuntu.com
```

## See Also

- [Trust Model](./trust-model.md) - How trust scores work
- [Tap Publishing](../usage/tap-publishing.md) - Signing packages
