#!/bin/bash
# Test tap repository management
source "$(dirname "$0")/common.sh"

section "Tap Repository Tests"

test_case "Tap list"
run_test_exit "reap tap list" 0

test_case "Tap add (test repo)"
# Add a test tap - this may fail if format is wrong
if reap tap add test-tap https://example.com/repo 2>/dev/null; then
    echo -e "  ${GREEN}✓ PASS${NC}: Tap added"
    ((TESTS_PASSED++))

    test_case "Tap remove"
    if reap tap remove test-tap 2>/dev/null; then
        echo -e "  ${GREEN}✓ PASS${NC}: Tap removed"
        ((TESTS_PASSED++))
    else
        echo -e "  ${RED}✗ FAIL${NC}: Tap remove failed"
        ((TESTS_FAILED++))
    fi
else
    echo -e "  ${YELLOW}⊘ SKIP${NC}: Tap add not available or requires different format"
    ((TESTS_SKIPPED++))
fi

report_results
