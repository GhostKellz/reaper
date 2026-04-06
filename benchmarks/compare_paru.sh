#!/bin/bash
#
# Benchmark script to compare Reaper performance against paru/yay
#
# Usage: ./compare_paru.sh [search|info|update|all]
#
# Prerequisites:
#   - cargo build --release (for reap)
#   - paru and/or yay installed
#

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Path to reap binary
REAP_BIN="${REAP_BIN:-./target/release/reap}"

# Check if reap is built
if [[ ! -x "$REAP_BIN" ]]; then
    echo -e "${YELLOW}Building reap in release mode...${NC}"
    cargo build --release
fi

# Test packages for benchmarks
SEARCH_QUERIES=("firefox" "neovim" "visual-studio" "discord" "spotify")
INFO_PACKAGES=("yay" "neovim" "firefox" "linux")

# Helper function to time a command
benchmark() {
    local name="$1"
    shift
    local cmd="$@"

    echo -e "${BLUE}Running: $name${NC}"
    echo "  Command: $cmd"

    # Run 3 times and take average
    local total=0
    for i in 1 2 3; do
        local start=$(date +%s.%N)
        eval "$cmd" > /dev/null 2>&1 || true
        local end=$(date +%s.%N)
        local elapsed=$(echo "$end - $start" | bc)
        total=$(echo "$total + $elapsed" | bc)
        echo "    Run $i: ${elapsed}s"
    done

    local avg=$(echo "scale=3; $total / 3" | bc)
    echo -e "  ${GREEN}Average: ${avg}s${NC}"
    echo ""
}

# Clear caches for fair comparison
clear_caches() {
    echo -e "${YELLOW}Clearing caches...${NC}"
    rm -rf ~/.cache/reaper/pkgbuild/* 2>/dev/null || true
    rm -rf ~/.cache/reaper/search/* 2>/dev/null || true
    rm -rf ~/.cache/paru/clone/* 2>/dev/null || true
    echo ""
}

# Benchmark search operations
benchmark_search() {
    echo -e "${GREEN}=== Search Benchmark ===${NC}"
    echo ""

    for query in "${SEARCH_QUERIES[@]}"; do
        echo -e "${YELLOW}Query: $query${NC}"

        # Reap
        if [[ -x "$REAP_BIN" ]]; then
            benchmark "reap search" "$REAP_BIN search $query"
        fi

        # Paru
        if command -v paru &> /dev/null; then
            benchmark "paru -Ss" "paru -Ss $query"
        fi

        # Yay
        if command -v yay &> /dev/null; then
            benchmark "yay -Ss" "yay -Ss $query"
        fi

        echo "---"
    done
}

# Benchmark info/query operations
benchmark_info() {
    echo -e "${GREEN}=== Package Info Benchmark ===${NC}"
    echo ""

    for pkg in "${INFO_PACKAGES[@]}"; do
        echo -e "${YELLOW}Package: $pkg${NC}"

        # Reap (using aur fetch for PKGBUILD info)
        if [[ -x "$REAP_BIN" ]]; then
            benchmark "reap aur fetch" "$REAP_BIN aur fetch $pkg"
        fi

        # Paru
        if command -v paru &> /dev/null; then
            benchmark "paru -Si" "paru -Si $pkg"
        fi

        # Yay
        if command -v yay &> /dev/null; then
            benchmark "yay -Si" "yay -Si $pkg"
        fi

        echo "---"
    done
}

# Benchmark update check
benchmark_update() {
    echo -e "${GREEN}=== Update Check Benchmark ===${NC}"
    echo ""

    # Reap
    if [[ -x "$REAP_BIN" ]]; then
        benchmark "reap update" "$REAP_BIN update"
    fi

    # Paru
    if command -v paru &> /dev/null; then
        benchmark "paru -Qu" "paru -Qu"
    fi

    # Yay
    if command -v yay &> /dev/null; then
        benchmark "yay -Qu" "yay -Qu"
    fi
}

# Benchmark with warm cache
benchmark_warm_cache() {
    echo -e "${GREEN}=== Warm Cache Operations ===${NC}"
    echo ""

    # First, warm the cache
    echo -e "${YELLOW}Warming cache...${NC}"
    "$REAP_BIN" search firefox > /dev/null 2>&1 || true
    paru -Ss firefox > /dev/null 2>&1 || true
    yay -Ss firefox > /dev/null 2>&1 || true
    echo ""

    # Now benchmark with warm cache
    echo -e "${YELLOW}Benchmarking with warm cache...${NC}"

    if [[ -x "$REAP_BIN" ]]; then
        benchmark "reap search (warm)" "$REAP_BIN search firefox"
    fi

    if command -v paru &> /dev/null; then
        benchmark "paru -Ss (warm)" "paru -Ss firefox"
    fi

    if command -v yay &> /dev/null; then
        benchmark "yay -Ss (warm)" "yay -Ss firefox"
    fi
}

# Generate summary report
generate_report() {
    echo -e "${GREEN}=== Benchmark Summary ===${NC}"
    echo ""
    echo "System: $(uname -a)"
    echo "Reap version: $($REAP_BIN --version 2>/dev/null || echo 'unknown')"
    if command -v paru &> /dev/null; then
        echo "Paru version: $(paru --version | head -1)"
    fi
    if command -v yay &> /dev/null; then
        echo "Yay version: $(yay --version | head -1)"
    fi
    echo "Date: $(date)"
    echo ""
}

# Main
main() {
    local mode="${1:-all}"

    generate_report

    case "$mode" in
        search)
            clear_caches
            benchmark_search
            ;;
        info)
            clear_caches
            benchmark_info
            ;;
        update)
            benchmark_update
            ;;
        warm)
            benchmark_warm_cache
            ;;
        all)
            clear_caches
            benchmark_search
            benchmark_info
            benchmark_update
            benchmark_warm_cache
            ;;
        *)
            echo "Usage: $0 [search|info|update|warm|all]"
            exit 1
            ;;
    esac

    echo -e "${GREEN}Benchmarks complete!${NC}"
}

main "$@"
