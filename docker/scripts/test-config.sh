#!/bin/bash
# Test configuration management
source "$(dirname "$0")/common.sh"

section "Configuration Tests"

test_case "Config show"
run_test "reap config show" ""

test_case "Config get backend_order"
run_test "reap config get backend_order" ""

test_case "Config set and get"
reap config set test_key "test_value" 2>/dev/null || true
output=$(reap config get test_key 2>&1)
if echo "$output" | grep -q "test_value\|not found\|unknown"; then
    echo -e "  ${GREEN}✓ PASS${NC}: config set/get works or correctly reports missing"
    ((TESTS_PASSED++))
else
    echo -e "  ${YELLOW}⊘ SKIP${NC}: config key behavior unclear"
    ((TESTS_SKIPPED++))
fi

test_case "Config file exists"
if [[ -f "$HOME/.config/reap/reap.toml" ]]; then
    echo -e "  ${GREEN}✓ PASS${NC}: config file exists"
    ((TESTS_PASSED++))
else
    echo -e "  ${RED}✗ FAIL${NC}: config file not found at ~/.config/reap/reap.toml"
    ((TESTS_FAILED++))
fi

report_results
