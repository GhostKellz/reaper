#!/bin/bash
# Reaper vs paru/yay benchmark comparison
# Run from project root: ./benchmarks/compare.sh

set -e

REAP_BIN="${REAP_BIN:-reap}"
PARU_BIN="${PARU_BIN:-paru}"

echo "========================================"
echo "Reaper Benchmark Suite"
echo "========================================"
echo ""

# Check if tools are available
if ! command -v "$REAP_BIN" &> /dev/null; then
    echo "Error: reap not found. Set REAP_BIN or add to PATH"
    exit 1
fi

if ! command -v "$PARU_BIN" &> /dev/null; then
    echo "Warning: paru not found, skipping comparison"
    SKIP_PARU=1
fi

echo "=== Search Benchmark (AUR) ==="
echo "Searching for 'firefox'..."
echo ""

echo "reap search firefox:"
time $REAP_BIN search firefox > /dev/null 2>&1
echo ""

if [ -z "$SKIP_PARU" ]; then
    echo "paru -Ss firefox:"
    time $PARU_BIN -Ss firefox > /dev/null 2>&1
    echo ""
fi

echo "=== Update Check Benchmark ==="
echo ""

echo "reap update:"
time $REAP_BIN update 2>&1 | tail -3
echo ""

if [ -z "$SKIP_PARU" ]; then
    echo "paru -Qu:"
    time $PARU_BIN -Qu 2>&1 | tail -3
    echo ""
fi

echo "=== Cache Performance ==="
echo ""

echo "First search (cold cache):"
$REAP_BIN perf clear-cache > /dev/null 2>&1 || true
time $REAP_BIN search neovim > /dev/null 2>&1
echo ""

echo "Second search (warm cache):"
time $REAP_BIN search neovim > /dev/null 2>&1
echo ""

echo "=== Flatpak Search (if available) ==="
if command -v flatpak &> /dev/null; then
    echo "reap flatpak search firefox:"
    time $REAP_BIN flatpak search firefox > /dev/null 2>&1 || echo "(flatpak search failed)"
    echo ""
else
    echo "Flatpak not installed, skipping"
fi

echo "========================================"
echo "Benchmark complete"
echo "========================================"
