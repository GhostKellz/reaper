# 🚀 REAP Roadmap

`reap` is a modern, async-first AUR helper written in Rust, built to replace tools like `yay` and `paru` — with better performance, extensibility, and a clean CLI/TUI experience.

---

## ✅ Minimum Viable Product (MVP)

Basic CLI functionality powered by `std::process::Command`:

- [x] `reap -S <pkg>` – Install AUR or repo package
- [x] `reap -R <pkg>` – Remove a package
- [x] `reap -Syu` – Sync and upgrade packages
- [x] `reap -U <file>` – Install local `.zst` or `.pkg.tar.zst`
- [x] `reap -Ss <term>` – Search AUR (via JSON-RPC)
- [x] Async execution using `tokio`
- [x] Basic error handling and logging

---

## 🔧 Near-Term Enhancements

More control, fewer dependencies:

- [ ] Parallel AUR package install (`--parallel`)
- [ ] Drop reliance on `yay` or `paru`
- [ ] Manual PKGBUILD retrieval from AUR
- [ ] Makepkg integration: `makepkg -si`
- [ ] Support Flatpak, AppImage, and `.deb` (as backends)
- [ ] Dependency resolution and conflict detection
- [ ] Interactive prompts: confirm removals, edit PKGBUILDs

---

## 🧰 Intermediate Features

Modular design, performance improvements:

- [ ] Pluggable backends (`reap --backend aur`, `--backend flatpak`)
- [ ] Caching: PKGBUILDs, metadata, search results
- [ ] Persistent config (TOML/YAML under `~/.config/reap`)
- [ ] Logging and audit mode (`--log`, `--audit`)
- [ ] Async install queues with progress bars

---

## 🎨 User Experience (TUI)

Optional terminal UI using `ratatui` or similar:

- [ ] `reap tui` – Full TUI interface
- [ ] Search, install, queue, review updates interactively
- [ ] Real-time log pane, diff viewer for PKGBUILDs

---

## 🔐 Security & Validation

Built-in trust and transparency:

- [ ] GPG key management (`--import-gpg`, `--check-gpg`)
- [ ] Package rollback (`--rollback`)
- [ ] Keyserver validation
- [ ] Audit mode to show upstream changes

---

## 🧪 Stretch & Experimental Ideas

Long-term exploration:

- [ ] Multi-distro support (e.g. `reap` inside containers for Debian/Fedora)
- [ ] AUR diff audit (compare PKGBUILD changes)
- [ ] Reap script mode: install from JSON manifest
- [ ] Headless mode for CI/CD systems
- [ ] WASM-based sandboxing for PKGBUILD parsing

---

## 💬 Community & Contribution

- [ ] `reap --version` and `--about` with repo link
- [ ] CONTRIBUTING.md for onboarding devs
- [ ] Plugin system for power users
- [ ] Discord community

---

## 📅 Status

Current focus: **MVP completion and transition away from yay/paru dependencies.**  
Target: **Self-contained, reliable AUR helper with fast, async-native execution.**

---

> Built with 🦀 Rust, 💻 by @ghostkellz
