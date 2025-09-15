# 🧪 Reaper v0.3.0 Release Validation Checklist

## 1. Install Scenario Validation

| Mode      | Command                                 | Expected Behavior                                                      |
|-----------|-----------------------------------------|-----------------------------------------------------------------------|
| ⚙️ Default | `reap --install foo`                    | Installs from tap or AUR, verifies sig if present, logs status        |
| 🚀 Fast    | `reap --install foo --fast`             | Skips sig, diff, and dep resolution, installs ASAP                    |
| 🔐 Strict  | `reap --install foo --strict`           | Aborts if sig missing or invalid                                      |
| 🚫 Insecure| `reap --install foo --insecure`         | Skips all sig checks without warning                                  |
| 🔁 Upgrade | `reap upgrade`                          | Parallel upgrade, respects fast/strict modes                          |
| 🔍 Search  | `reap search foo`                       | Shows results with [tap:foo], [aur], [flatpak] tags                  |
| 🔄 Rollback| `reap rollback foo`                     | Restores last install from backup                                     |
| 🧪 Tap-only| `reap install --backend tap foo`        | Only install if found in tap                                          |

## 2. Security & GPG Validation
- Tap install fails on invalid signature (unless --insecure)
- publisher.toml detected and displayed
- GPG key is fetched if not local, fingerprint matches publisher.toml

## 3. CI Matrix (see .github/workflows/main.yml)
- Lint: `cargo clippy --all-targets --all-features`
- Format: `cargo fmt --all -- --check`
- Test: `cargo test --all`
- Build: --release, features: default, flatpak, cache
- Platforms: x86_64-unknown-linux-gnu, aarch64, optionally musl
- Shell completion and tap repo detection logic

## 4. Release Packaging
- [ ] Build `reap-x86_64.tar.gz` and (optionally) `reap-aarch64.tar.gz`
- [ ] PKGBUILD for manual install or AUR submit
- [ ] completion/zsh/_reap, bash/reap, fish/reap.fish

## 5. Final Prep
- [ ] CHANGELOG.md with all changes since v0.2
- [ ] Update ROADMAP.md and TODOs for v0.4/v1.0
- [ ] Update INSTALL.md with binary/manual install and completions
- [ ] (Optional) Add benchmarks.md for performance comparisons
