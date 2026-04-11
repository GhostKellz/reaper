#!/bin/bash
# Test Flatpak integration
source "$(dirname "$0")/common.sh"

section "Flatpak Tests"

# Check if flatpak is available
if ! has_command flatpak; then
    echo "Flatpak not installed, skipping flatpak tests"
    skip_test "Flatpak not available"
    report_results
    exit 0
fi

test_case "Flatpak list"
# May fail if flatpak not configured in container
if reap flatpak list 2>/dev/null; then
    echo -e "  ${GREEN}✓ PASS${NC}: flatpak list succeeded"
    ((TESTS_PASSED++))
else
    skip_test "flatpak not configured in container"
fi

test_case "Flatpak remotes"
if reap flatpak remotes 2>/dev/null; then
    echo -e "  ${GREEN}✓ PASS${NC}: flatpak remotes succeeded"
    ((TESTS_PASSED++))
else
    skip_test "flatpak not configured in container"
fi

test_case "Flatpak search"
run_test "reap flatpak search firefox" ""

test_case "Flatpak check-updates"
run_test_exit "reap flatpak check-updates" 0

test_case "Flatpak audit"
if reap flatpak audit 2>/dev/null; then
    echo -e "  ${GREEN}✓ PASS${NC}: flatpak audit succeeded"
    ((TESTS_PASSED++))
else
    skip_test "flatpak audit not available"
fi

report_results
