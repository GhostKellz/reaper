# Reap Release Makefile
VERSION = 0.5.0
PACKAGE_NAME = reap
ARCH := $(shell uname -m)

# Directories
BUILD_DIR = target/release
RELEASE_DIR = release
DIST_DIR = $(RELEASE_DIR)/dist

# Files
BINARY = $(BUILD_DIR)/$(PACKAGE_NAME)
TARBALL = $(DIST_DIR)/$(PACKAGE_NAME)-$(ARCH).tar.gz
PKGBUILD = $(RELEASE_DIR)/PKGBUILD

.PHONY: all build test clean release dist install uninstall package-aur

all: build

# Build the release binary
build:
	@echo "Building reap v$(VERSION) for $(ARCH)..."
	cargo build --release --locked --all-features

# Run tests
test:
	@echo "Running tests..."
	cargo test --release --locked

# Clean build artifacts
clean:
	@echo "Cleaning build artifacts..."
	cargo clean
	rm -rf $(DIST_DIR)

# Generate shell completions
completions: build
	@echo "Generating shell completions..."
	@mkdir -p $(DIST_DIR)
	$(BINARY) completion bash > $(DIST_DIR)/$(PACKAGE_NAME).bash 2>/dev/null || true
	$(BINARY) completion zsh > $(DIST_DIR)/$(PACKAGE_NAME).zsh 2>/dev/null || true
	$(BINARY) completion fish > $(DIST_DIR)/$(PACKAGE_NAME).fish 2>/dev/null || true

# Create distribution tarball
dist: build completions
	@echo "Creating distribution tarball..."
	@mkdir -p $(DIST_DIR)
	
	# Copy binary
	cp $(BINARY) $(DIST_DIR)/
	
	# Copy documentation
	cp README.md FEATURES.md SECURITY.md CHANGELOG.md LICENSE $(DIST_DIR)/
	
	# Copy install script
	cp $(RELEASE_DIR)/install.sh $(DIST_DIR)/
	
	# Create tarball
	cd $(DIST_DIR) && tar -czf ../$(PACKAGE_NAME)-$(ARCH).tar.gz *
	
	@echo "Distribution created: $(RELEASE_DIR)/$(PACKAGE_NAME)-$(ARCH).tar.gz"

# Create source tarball for AUR
source-dist:
	@echo "Creating source distribution..."
	@mkdir -p $(DIST_DIR)
	git archive --format=tar.gz --prefix=$(PACKAGE_NAME)-$(VERSION)/ HEAD > $(DIST_DIR)/$(PACKAGE_NAME)-$(VERSION).tar.gz
	@echo "Source distribution created: $(DIST_DIR)/$(PACKAGE_NAME)-$(VERSION).tar.gz"

# Update PKGBUILD checksum
update-pkgbuild: source-dist
	@echo "Updating PKGBUILD checksum..."
	@cd $(DIST_DIR) && \
	CHECKSUM=$$(sha256sum $(PACKAGE_NAME)-$(VERSION).tar.gz | cut -d' ' -f1) && \
	sed -i "s/sha256sums=('SKIP')/sha256sums=('$$CHECKSUM')/" ../PKGBUILD
	@echo "PKGBUILD updated with checksum"

# Package for AUR
package-aur: update-pkgbuild
	@echo "Creating AUR package..."
	@mkdir -p $(DIST_DIR)/aur
	cp $(PKGBUILD) $(DIST_DIR)/aur/
	@echo "AUR package ready in $(DIST_DIR)/aur/"

# Install locally
install: build completions
	@echo "Installing reap..."
	sudo install -Dm755 $(BINARY) /usr/local/bin/$(PACKAGE_NAME)
	
	# Install completions
	sudo mkdir -p /usr/share/bash-completion/completions
	sudo mkdir -p /usr/share/zsh/site-functions
	sudo mkdir -p /usr/share/fish/vendor_completions.d
	
	sudo install -Dm644 $(DIST_DIR)/$(PACKAGE_NAME).bash /usr/share/bash-completion/completions/$(PACKAGE_NAME) 2>/dev/null || true
	sudo install -Dm644 $(DIST_DIR)/$(PACKAGE_NAME).zsh /usr/share/zsh/site-functions/_$(PACKAGE_NAME) 2>/dev/null || true
	sudo install -Dm644 $(DIST_DIR)/$(PACKAGE_NAME).fish /usr/share/fish/vendor_completions.d/$(PACKAGE_NAME).fish 2>/dev/null || true
	
	@echo "Installation complete!"

# Uninstall
uninstall:
	@echo "Uninstalling reap..."
	sudo rm -f /usr/local/bin/$(PACKAGE_NAME)
	sudo rm -f /usr/share/bash-completion/completions/$(PACKAGE_NAME)
	sudo rm -f /usr/share/zsh/site-functions/_$(PACKAGE_NAME)
	sudo rm -f /usr/share/fish/vendor_completions.d/$(PACKAGE_NAME).fish
	@echo "Uninstallation complete!"

# Full release workflow
release: clean test dist source-dist package-aur
	@echo "Release v$(VERSION) ready!"
	@echo "Files created:"
	@echo "  - $(RELEASE_DIR)/$(PACKAGE_NAME)-$(ARCH).tar.gz (binary distribution)"
	@echo "  - $(DIST_DIR)/$(PACKAGE_NAME)-$(VERSION).tar.gz (source distribution)"
	@echo "  - $(DIST_DIR)/aur/PKGBUILD (AUR package)"

# Development helpers
dev-install: build
	@echo "Installing development build..."
	sudo install -Dm755 $(BINARY) /usr/local/bin/$(PACKAGE_NAME)-dev

check-version:
	@echo "Current version: $(VERSION)"
	@echo "Binary version: $$($(BINARY) --version 2>/dev/null || echo 'not built')"

help:
	@echo "Reap Release Makefile"
	@echo "Usage: make [target]"
	@echo ""
	@echo "Targets:"
	@echo "  build         Build release binary"
	@echo "  test          Run tests"
	@echo "  clean         Clean build artifacts"
	@echo "  completions   Generate shell completions"
	@echo "  dist          Create binary distribution tarball"
	@echo "  source-dist   Create source distribution tarball"
	@echo "  package-aur   Create AUR package with updated checksum"
	@echo "  install       Install locally"
	@echo "  uninstall     Uninstall"
	@echo "  release       Full release workflow"
	@echo "  dev-install   Install development build"
	@echo "  check-version Show version information"
	@echo "  help          Show this help"