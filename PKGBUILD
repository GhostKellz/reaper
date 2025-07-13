# Maintainer: Christopher Kelley <ckelley@ghostkellz.sh>
# Contributor: CK Technology

pkgname=reaper
pkgver=0.2.0
pkgrel=1
pkgdesc="Unified AUR Helper & Build System with Ghost kernel/driver support"
arch=('x86_64')
url="https://github.com/cktech/reaper"
license=('MIT')
depends=('pacman' 'git')
makedepends=('zig>=0.13.0')
optdepends=(
    'zmake: Enhanced build system integration'
    'linux-ghost-headers: For kernel module building'
    'base-devel: For AUR package building'
    'asp: For repository package building'
    'flatpak: Flatpak backend support'
    'nvidia-dkms: For GhostNV driver building'
)
provides=('reap')
backup=('etc/reaper/config.toml')
source=("$pkgname-$pkgver.tar.gz")
sha256sums=('SKIP')

build() {
    cd "$srcdir/reaper-$pkgver"
    
    # Build with Zig in release mode
    zig build -Doptimize=ReleaseFast
}

check() {
    cd "$srcdir/reaper-$pkgver"
    
    # Test basic functionality
    ./zig-out/bin/reap help
    ./zig-out/bin/reap make --help
    ./zig-out/bin/reap make kernel --help
    ./zig-out/bin/reap make ghostnv --help
}

package() {
    cd "$srcdir/reaper-$pkgver"
    
    # Install main binary
    install -Dm755 zig-out/bin/reap "$pkgdir/usr/bin/reap"
    
    # Create compatibility symlinks
    ln -s reap "$pkgdir/usr/bin/reaper"
    ln -s reap "$pkgdir/usr/bin/reap-pkg"
    
    # Install zsh completions
    install -Dm644 completions/_reap "$pkgdir/usr/share/zsh/site-functions/_reap"
    
    # Install documentation
    install -Dm644 README.md "$pkgdir/usr/share/doc/$pkgname/README.md"
    install -Dm644 CLAUDE.md "$pkgdir/usr/share/doc/$pkgname/CLAUDE.md"
    install -Dm644 FEATURES.md "$pkgdir/usr/share/doc/$pkgname/FEATURES.md"
    install -Dm644 COMMANDS.md "$pkgdir/usr/share/doc/$pkgname/COMMANDS.md"
    install -Dm644 MAKE_COMMANDS.md "$pkgdir/usr/share/doc/$pkgname/MAKE_COMMANDS.md"
    install -Dm644 SECURITY.md "$pkgdir/usr/share/doc/$pkgname/SECURITY.md"
    
    # Install license
    install -Dm644 LICENSE "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
    
    # Create config directory structure
    install -dm755 "$pkgdir/etc/reaper"
    
    # Install default configuration
    cat > "$pkgdir/etc/reaper/config.toml" << 'EOF'
# Reaper Configuration File
# CK Technology - Ghost Build System

[general]
# Enable parallel operations
parallel = true
# Number of build jobs (0 = auto-detect)
jobs = 0
# Enable colored output
color = true

[security]
# Enable package trust scoring
trust_scoring = true
# Minimum trust score for automatic installation
min_trust_score = 4.0
# Enable GPG verification
gpg_verify = true

[build]
# Default kernel profile for 'reap make kernel'
default_kernel_profile = "ghost"
# Default NVIDIA mode for 'reap make ghostnv'  
default_nvidia_mode = "ghost"
# Use Zig compiler when available
prefer_zig_cc = true

[backends]
# Enable AUR backend
aur = true
# Enable Pacman backend  
pacman = true
# Enable Flatpak backend (if available)
flatpak = true

[make]
# Enable BORE scheduler patches by default
enable_bore = true
# Enable CachyOS performance patches
enable_cachy = true
# Enable AMD optimizations
enable_amd_opts = true
# Enable Elgato streaming fixes
enable_elgato_fixes = true
# Enable NVENC optimizations for GhostNV
enable_nvenc = true
# Enable NVAFX audio processing
enable_nvafx = true
EOF
}

post_install() {
    echo ":: Reaper has been installed!"
    echo ""
    echo "🚀 Quick Start:"
    echo "   reap search firefox      # Search for packages"
    echo "   reap install yay-bin     # Install AUR packages"
    echo "   reap make kernel --help  # Build custom kernels"
    echo "   reap make ghostnv --help # Build NVIDIA drivers"
    echo ""
    echo "🔧 Advanced Features:"
    echo "   reap make kernel --profile amd-x3d    # AMD X3D optimized kernel"
    echo "   reap make ghostnv --enable-audio-cancel  # NVIDIA with audio features"
    echo "   reap make iso --profile gaming        # Custom Arch ISO"
    echo "   reap trust package-name               # Check package trust score"
    echo ""
    echo "📝 Configuration: /etc/reaper/config.toml"
    echo "📚 Documentation: /usr/share/doc/reaper/"
    echo ""
    echo "⚡ Restart your shell to enable zsh completions"
    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "  🎮 Ghost Build System by CK Technology"
    echo "  📧 Christopher Kelley <ckelley@ghostkellz.sh>"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
}

post_upgrade() {
    echo ":: Reaper has been upgraded!"
    echo ":: Check /usr/share/doc/reaper/ for new features"
    echo ":: Configuration: /etc/reaper/config.toml"
}

pre_remove() {
    echo ":: Removing reaper..."
    echo ":: Configuration will be preserved in /etc/reaper/"
}