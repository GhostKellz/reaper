# Security Guide

Reaper provides security information to help you make informed decisions about package installation.

## Trust Model: Advisory Only

Reaper uses an **advisory trust score** model:

- Trust scores and warnings are **informational**
- Installation is **never blocked** based on trust score
- You are shown security data and can decide how to proceed
- Tap package signature verification is a separate install policy and may block
  unsigned or invalid tap packages unless `--insecure` is used

This approach respects user autonomy while providing security awareness.

## Trust Scores

Every package receives a trust score (0-10):

| Score | Level | Meaning |
|-------|-------|---------|
| 8-10 | TRUSTED | Verified signatures, known publisher |
| 6-7.9 | VERIFIED | Good community standing |
| 4-5.9 | CAUTION | Review recommended |
| 2-3.9 | RISKY | Multiple concerns found |
| 0-1.9 | UNSAFE | Serious issues detected |

Check a package's trust score:

```bash
reap trust score <package>
```

## What Affects Trust Score

- **GPG signature** - Valid signature adds to score
- **Publisher verification** - Known maintainer adds to score
- **Community data** - AUR votes and popularity
- **PKGBUILD analysis** - Suspicious patterns reduce score

## GPG Verification

For tap repositories with signed packages:

| Scenario | Behavior |
|----------|----------|
| Valid signature | Verified, shows badge |
| Invalid signature | Install aborts unless `--insecure` is used |
| No signature | Install aborts unless `--insecure` is used |

Use `--strict` to require signatures:
```bash
reap install package --strict
```

Use `--insecure` to skip verification entirely (not recommended):
```bash
reap install package --insecure
```

## AUR Packages

AUR packages typically don't have GPG signatures - this is normal for AUR. Trust for AUR packages is based on maintainer reputation, community votes, and PKGBUILD analysis.

## Security Best Practices

1. **Review PKGBUILDs** before installing unfamiliar packages
2. **Check trust scores** for packages from unknown sources
3. **Use `--diff`** to see PKGBUILD changes before upgrades
4. **Import trusted keys** for publishers you trust

```bash
# Review PKGBUILD before install
reap aur fetch package
reap install package --diff
```

## Reporting Security Issues

For security vulnerabilities in Reaper itself:
- Do not open public issues
- Contact the maintainers directly
- Allow time for responsible disclosure

## Documentation

- [Trust Model Details](docs/security/trust-model.md)
- [GPG Verification](docs/security/gpg-verification.md)
- [Tap Publishing](docs/usage/tap-publishing.md)
