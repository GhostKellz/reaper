# 🛠️ INSTALL.md — Building and Installing Reap

## Prerequisites
- Rust (latest stable, install via https://rustup.rs)
- Git
- GPG (for signature verification)
- Flatpak (optional, for Flatpak support)

## Build from Source

```bash
git clone https://github.com/GhostKellz/reaper.git
cd reaper
cargo build --release
```

## Install

```bash
cargo install --path .
# Or copy the binary manually:
cp target/release/reap ~/.local/bin/
```

## Test

```bash
cargo test
```

## Enable Features

- Enable caching (default):
  - `cargo build --release --features cache`
- Enable Lua scripting for hooks (planned):
  - `cargo build --release --features lua`

## Run

```bash
reap --help
```

- All install/upgrade logic is now async/parallel and does not use yay/paru fallback.
- See README.md and COMMANDS.md for the latest CLI options and features. 
## Binary Releases

Download the latest `reap-x86_64.tar.gz` from the GitHub Releases page and extract:

```bash
tar -xvf reap-x86_64.tar.gz
sudo cp reap /usr/local/bin/
```

## Shell Completions

Bash:
```bash
reap completion bash > /etc/bash_completion.d/reap
```
Zsh:
```bash
reap completion zsh > /usr/share/zsh/site-functions/_reap
```
Fish:
```bash
reap completion fish > ~/.config/fish/completions/reap.fish
```
