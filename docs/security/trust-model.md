# Trust Model

Reaper uses an **advisory-only** trust model. Trust scores inform you about package security but never block installation.

## Philosophy

Reaper shows you security information so you can make informed decisions. It will:

- Display trust scores and security warnings
- Show GPG signature verification status
- Highlight suspicious PKGBUILD patterns

It will **not**:

- Block installation based on trust score
- Require confirmation for low-trust packages
- Force you to use `--insecure` for unverified packages

## Trust Scores

Packages receive a trust score from 0-10:

| Score | Badge | Meaning |
|-------|-------|---------|
| 8.0-10.0 | TRUSTED | High confidence, verified signatures |
| 6.0-7.9 | VERIFIED | Good standing, some verification |
| 4.0-5.9 | CAUTION | Review recommended |
| 2.0-3.9 | RISKY | Multiple concerns |
| 0.0-1.9 | UNSAFE | Serious issues detected |

## Score Factors

Trust scores consider:

- **GPG signature validity** - Is the package signed with a valid key?
- **Publisher verification** - Is the maintainer known and trusted?
- **Community data** - AUR votes and popularity
- **PKGBUILD analysis** - Any suspicious patterns detected?

## Viewing Trust Information

```bash
# Check trust score for a package
reap trust score firefox

# Output shows score and any security flags
```

## AUR Packages

AUR packages typically don't have GPG signatures. This is expected and doesn't indicate a security problem - it's how AUR works.

For AUR packages, trust is based on:
- Maintainer reputation
- Community votes
- PKGBUILD analysis
- Package age and history

## Tap Packages

Tap repositories can provide GPG-signed packages. For taps:

- Signatures are verified when present
- Missing signatures show a warning (not a block)
- Use `--strict` to require signatures
- Use `--insecure` to skip all verification

## Security Flags

Packages may have security flags indicating concerns:

- `UnverifiedSignature` - No valid GPG signature
- `UnknownPublisher` - Maintainer not in trusted list
- `SuspiciousContent` - PKGBUILD has concerning patterns
- `NetworkAccess` - Package makes network requests during build

These are informational - they don't prevent installation.

## See Also

- [GPG Verification](./gpg-verification.md) - Signature verification details
- [Tap Publishing](../usage/tap-publishing.md) - How to sign tap packages
