# Resolved Advisories

Security advisories that previously appeared in `cargo audit` output and have
since been cleared by dependency updates.

| Advisory | Crate | Issue | Resolved by | Date |
|----------|-------|-------|-------------|------|
| RUSTSEC-2026-0002 | `lru` | `IterMut` violates Stacked Borrows | Bumped `ratatui` to 0.30.x, which pulls a fixed `lru` line | 2026-06-17 |
| RUSTSEC-2025-0119 | `number_prefix` | Crate unmaintained | Bumped `indicatif` to 0.18.x, which uses `unit-prefix` instead | 2026-06-17 |
| RUSTSEC-2025-0009 | `ring` / TLS stack | Historical TLS dependency advisory | Removed direct TLS stack dependencies and let `reqwest` manage modern TLS | 2026-06-17 |
| RUSTSEC-2025-0010 | `rustls` / TLS stack | Historical TLS dependency advisory | Removed direct TLS stack dependencies and let `reqwest` manage modern TLS | 2026-06-17 |
| RUSTSEC-2024-0336 | `webpki` / TLS stack | Historical TLS dependency advisory | Removed direct TLS stack dependencies and let `reqwest` manage modern TLS | 2026-06-17 |

After the current dependency refresh, `cargo audit` reports no accepted
vulnerabilities.
