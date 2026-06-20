# Reaper Test Environment - Arch Linux
FROM archlinux:latest

# Update system and install dependencies
RUN pacman -Syu --noconfirm && \
    pacman -S --noconfirm \
    base-devel \
    git \
    rust \
    cargo \
    flatpak \
    gnupg \
    openssh \
    jq \
    openssl \
    pkgconf \
    fakeroot \
    sudo \
    && pacman -Scc --noconfirm

# Create non-root test user (makepkg requires non-root)
RUN useradd -m -G wheel -s /bin/bash testuser && \
    echo "testuser ALL=(ALL) NOPASSWD: ALL" >> /etc/sudoers

# Copy reaper source
WORKDIR /reaper
COPY . .

# Fix ownership for build
RUN chown -R testuser:testuser /reaper

# Build as testuser
USER testuser
RUN cargo build --release --features cache

# Install reaper (handle both target paths)
USER root
RUN cp /reaper/target/release/reap /usr/local/bin/ 2>/dev/null || \
    cp /reaper/target/x86_64-unknown-linux-gnu/release/reap /usr/local/bin/ && \
    chmod 755 /usr/local/bin/reap

# Setup test user environment
USER testuser
WORKDIR /home/testuser

# Initialize reaper config
RUN mkdir -p ~/.config/reap && \
    reap doctor || true

# Make test scripts executable
USER root
RUN chmod +x /reaper/docker/scripts/*.sh 2>/dev/null || true

USER testuser
CMD ["/bin/bash"]
