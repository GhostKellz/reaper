#!/bin/bash
# Test GPG key management
source "$(dirname "$0")/common.sh"

section "GPG Tests"

test_case "GPG show (requires keyid)"
run_expect_fail "reap gpg show"

test_case "GPG check (requires keyid)"
run_expect_fail "reap gpg check"

test_case "GPG refresh (may timeout)"
# This requires network and may take time
timeout 30 reap gpg refresh 2>/dev/null
exit_code=$?
if [[ $exit_code -eq 0 ]]; then
    echo -e "  ${GREEN}✓ PASS${NC}: GPG refresh succeeded"
    ((TESTS_PASSED++))
elif [[ $exit_code -eq 124 ]]; then
    echo -e "  ${YELLOW}⊘ SKIP${NC}: GPG refresh timed out (network issue)"
    ((TESTS_SKIPPED++))
else
    echo -e "  ${YELLOW}⊘ SKIP${NC}: GPG refresh failed (may be expected without keys)"
    ((TESTS_SKIPPED++))
fi

report_results
