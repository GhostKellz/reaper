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

## Supply-Chain Detection

`reap security audit` and `reap security scan-all` analyze both the PKGBUILD and
its `.install` hook. Install hooks run on your machine with elevated privileges,
so they are a common hiding place for payloads.

Beyond the general risky-pattern scan, reaper flags build-time techniques seen in
real AUR supply-chain campaigns (e.g. the June 2026 "Atomic Arch" incident):

- **Bundled hook execution** - running scripts or binaries shipped under `src/hooks/`
- **JS package installs** - `npm`/`bun`/`pnpm`/`yarn` pulling and executing dependencies during build
- **npm lifecycle hooks** - `preinstall`/`postinstall` scripts that execute local code
- **Tor C2 endpoints** - `.onion` addresses
- **Temporary-file hosts** - paste/upload sites used for staging payloads

Detection is **behavioral** - it targets the techniques rather than specific
indicators, so it stays useful as payloads change. As with all trust signals,
these are advisory and never block installation. Always review PKGBUILD and
`.install` diffs before rebuilding a package, especially for adopted or
recently-changed AUR packages.

## See Also

- [GPG Verification](./gpg-verification.md) - Signature verification details
- [Tap Publishing](../usage/tap-publishing.md) - How to sign tap packages
