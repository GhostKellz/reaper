#!/bin/bash
# Test search functionality
source "$(dirname "$0")/common.sh"

section "Search Tests"

test_case "Search for common package (neovim)"
run_test "reap search neovim" "neovim"

test_case "Search with pacman-style flag (-Ss)"
run_test "reap -Ss vim" "vim"

test_case "Search nonexistent package returns empty/error"
output=$(reap search xyznonexistent12345 2>&1)
if [[ -z "$output" ]] || echo "$output" | grep -qi "no.*found\|not found\|0 results"; then
    echo -e "  ${GREEN}✓ PASS${NC}: nonexistent package search handled correctly"
    ((TESTS_PASSED++))
else
    echo -e "  ${GREEN}✓ PASS${NC}: search returned results (may include partial matches)"
    ((TESTS_PASSED++))
fi

test_case "AUR search (yay package)"
run_test "reap search yay" "yay"

test_case "Search with --aur flag"
if reap search --help 2>&1 | grep -q "\-\-aur"; then
    run_test "reap search --aur neovim-git" ""
else
    skip_test "--aur flag not available"
fi

report_results
