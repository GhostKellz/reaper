# Trust Model

Reaper uses an **advisory trust score** model. Trust scores inform you about package security but never block installation.

## Philosophy

Reaper shows you security information so you can make informed decisions. It will:

- Display trust scores and security warnings
- Show GPG signature verification status
- Highlight suspicious PKGBUILD patterns

It will **not**:

- Block installation based on trust score
- Require confirmation for low-trust packages
- Treat low community trust as a cryptographic failure

There is **one deliberate exception** to the advisory-only rule: high-confidence
infostealer evidence aborts the install by default (see
[Infostealer Hard-Block](#infostealer-hard-block) below). Tap package signature
verification is also a separate install policy: invalid or missing tap signatures
abort installation unless `--insecure` is used.

## Trust Scores

Packages receive a trust score from 0-10:

| Score | Badge | Meaning |
|-------|-------|---------|
| 8.0-10.0 | HIGH TRUST | Strong positive community signals |
| 6.0-7.9 | MODERATE | Good standing |
| 4.0-5.9 | LOW TRUST | Review recommended |
| 2.0-3.9 | RISKY | Multiple concerns |
| 0.0-1.9 | UNTRUSTED | Serious issues detected |

These labels reflect heuristic trust scores, not cryptographic verification.
Signature status is shown separately (`VERIFIED`/`SIGNED`) when a tap package
carries a valid signature.

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

- Signatures are required by default for install
- Missing or invalid signatures abort install
- Use `--strict` to require fully trusted signatures where verification is available
- Use `--insecure` to skip all verification

## Security Flags

Packages may have security flags indicating concerns:

- `UnverifiedSignature` - No valid GPG signature
- `UnknownPublisher` - Maintainer not in trusted list
- `SuspiciousFiles` - PKGBUILD has concerning patterns
- `NetworkAccess` - Package makes network requests during build

These trust flags are informational. Signature verification failures on tap
packages are handled separately by the install policy.

## Supply-Chain Detection

All heuristic auditing runs through a single engine (`src/audit.rs`). It analyzes
both the PKGBUILD and its `.install` hook together. Install hooks run on your
machine with elevated privileges, so they are a common hiding place for payloads.
`reap security audit <pkg|path>`, `reap security scan-all`, `reap audit <pkg>`,
and the install/upgrade review all share this engine, so findings are consistent
across commands. `reap security stats` reports the live rule counts.

Install, batch-install, and upgrade run a structured review for planned AUR
builds. The interactive findings and the high-risk confirmation prompt can be
silenced with `--skipreview`, but the review itself still runs so the infostealer
hard-block stays enforced. After a reviewed package is installed or upgraded,
reaper stores the accepted PKGBUILD **and `.install` hook** baseline under
`~/.local/share/reap/reviews/`. Future reviews show whether each is new,
unchanged, or changed since the last accepted review, with a compact changed-line
preview before the install confirmation gate. Because the `.install` hook runs as
root, any change to it is highlighted separately and prominently.
`--dry-run` prints the same review without writing review state.

Beyond the general risky-pattern scan, reaper flags build-time techniques seen in
real AUR supply-chain campaigns (e.g. the June 2026 "Atomic Arch" incident):

- **Bundled hook execution** - running scripts or binaries shipped under `src/hooks/`
- **JS package installs** - `npm`/`bun`/`pnpm`/`yarn` pulling and executing dependencies during build
- **npm lifecycle hooks** - `preinstall`/`postinstall` scripts that execute local code
- **Tor C2 endpoints** - `.onion` addresses
- **Temporary-file hosts** - paste/upload sites used for staging payloads
- **Source-integrity gaps** - non-HTTPS or raw-IP sources, unpinned VCS sources
  (no `#commit=`/`#tag=`), and network fetches hidden inside `build()`/`prepare()`
  that bypass checksum verification

Most detection is **behavioral** - it targets the techniques rather than specific
indicators, so it stays useful as payloads change. With the single exception of
the infostealer hard-block below, these signals are advisory and never block
installation. Always review PKGBUILD and `.install` diffs before rebuilding a
package, especially for adopted or recently-changed AUR packages.

## Infostealer Hard-Block

Infostealers are the most common real-world AUR payload: they read sensitive
credentials and send them to an attacker. Reaper detects this by **correlation**,
not by any single keyword:

- **Sensitive read** - SSH/GPG private keys, `.aws/credentials`, `.git-credentials`,
  browser login/cookie databases, crypto wallets, `/proc/self/environ`, etc.
- **Exfiltration** - `curl`/`wget` uploads or POSTs, `/dev/tcp` and reverse-shell
  idioms, Discord/Telegram/Slack webhooks, or upload hosts.
- **Obfuscation** - base64/hex-decoded payloads piped to a shell.

Confidence is assigned from how these co-occur:

| Confidence | Condition |
|------------|-----------|
| High | a sensitive read correlated with exfiltration, or obfuscation paired with either |
| Medium | a sensitive read or an exfiltration mechanism alone |
| Low | obfuscation alone |

**High confidence is the only condition under which reaper overrides you.** It
hard-stops `install`, `batch-install`, and `upgrade` by default. The override is
intentionally limited to `--insecure`; it is **not** bypassed by `--skipreview`
or `--noconfirm`. `batch-install` and `upgrade` have no `--insecure` flag, so the
block is absolute there - install the package individually if you have verified
it is a false positive.

Requiring both a sensitive read *and* an exfiltration path keeps false positives
low: a package that merely uses `curl`, or one literally named `chromium`, will
not trip the block on its own.

## Reviewing Diffs

```bash
# Show the current PKGBUILD and .install hook against the last reviewed
# baseline, followed by a risk/infostealer summary
reap diff <pkg>

# Same diff as a preflight before installing
reap install <pkg> --diff
```

## See Also

- [GPG Verification](./gpg-verification.md) - Signature verification details
- [Tap Publishing](../usage/tap-publishing.md) - How to sign tap packages
