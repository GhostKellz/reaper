#!/bin/bash
# Master test runner for reaper test suite
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
TOTAL_PASSED=0
TOTAL_FAILED=0
TOTAL_SKIPPED=0
FAILED_SUITES=()

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  Reaper Test Suite"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "Running on: $(uname -a)"
echo "Reaper version: $(reap --version 2>&1 || echo 'not installed')"
echo ""

# List of test scripts to run
TEST_SCRIPTS=(
    "test-cli-basic.sh"
    "test-search.sh"
    "test-info.sh"
    "test-config.sh"
    "test-profiles.sh"
    "test-trust.sh"
    "test-cache.sh"
    "test-gpg.sh"
    "test-tap.sh"
    "test-rollback.sh"
    "test-flatpak.sh"
)

for script in "${TEST_SCRIPTS[@]}"; do
    script_path="$SCRIPT_DIR/$script"

    if [[ -x "$script_path" ]]; then
        echo ""
        echo "Running: $script"
        echo "────────────────────────────────────────────────"

        if "$script_path"; then
            echo "Suite $script: PASSED"
        else
            echo "Suite $script: FAILED"
            FAILED_SUITES+=("$script")
        fi
    else
        echo "Skipping: $script (not found or not executable)"
    fi
done

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  Final Summary"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

if [[ ${#FAILED_SUITES[@]} -eq 0 ]]; then
    echo ""
    echo "All test suites passed!"
    exit 0
else
    echo ""
    echo "Failed suites:"
    for suite in "${FAILED_SUITES[@]}"; do
        echo "  - $suite"
    done
    exit 1
fi
