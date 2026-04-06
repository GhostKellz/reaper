# Release

Release packaging for Reaper.

## Contents

- `PKGBUILD` - AUR package build file
- `install.sh` - Automated installer
- `uninstall.sh` - Uninstaller
- `build.sh` - Release build script

## Installation

### Quick Install
```bash
curl -sSL https://raw.githubusercontent.com/GhostKellz/reaper/main/release/install.sh | bash
```

### Manual Install
```bash
tar -xzf reap-x86_64.tar.gz
sudo cp reap /usr/local/bin/
```

### Build from Source
```bash
git clone https://github.com/GhostKellz/reaper.git
cd reaper
cargo build --release
sudo cp target/release/reap /usr/local/bin/
```

### AUR Package
```bash
makepkg -si
```

## Building a Release

```bash
./release/build.sh
```

This creates release artifacts in `release/artifacts/`.

## Uninstall

```bash
./release/uninstall.sh
```
