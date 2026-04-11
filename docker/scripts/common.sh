#!/bin/bash
# Common test utilities for reaper test suite

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Counters
TESTS_PASSED=0
TESTS_FAILED=0
TESTS_SKIPPED=0
CURRENT_SECTION=""

# Print section header
section() {
    CURRENT_SECTION="$1"
    echo ""
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${BLUE}  $1${NC}"
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
}

# Print test case name
test_case() {
    echo -e "\n${YELLOW}▶ $1${NC}"
}

# Run a test and check output contains expected string
# Usage: run_test "command" "expected_substring"
run_test() {
    local cmd="$1"
    local expected="$2"
    local output
    local exit_code

    output=$(eval "$cmd" 2>&1) || exit_code=$?

    if echo "$output" | grep -q "$expected"; then
        echo -e "  ${GREEN}✓ PASS${NC}: $cmd"
        ((TESTS_PASSED++))
        return 0
    else
        echo -e "  ${RED}✗ FAIL${NC}: $cmd"
        echo -e "  Expected to contain: '$expected'"
        echo -e "  Got: $output"
        ((TESTS_FAILED++))
        return 1
    fi
}

# Run a test and check exact exit code
# Usage: run_test_exit "command" expected_exit_code
run_test_exit() {
    local cmd="$1"
    local expected_exit="$2"
    local exit_code=0

    eval "$cmd" >/dev/null 2>&1 || exit_code=$?

    if [[ $exit_code -eq $expected_exit ]]; then
        echo -e "  ${GREEN}✓ PASS${NC}: $cmd (exit $exit_code)"
        ((TESTS_PASSED++))
        return 0
    else
        echo -e "  ${RED}✗ FAIL${NC}: $cmd"
        echo -e "  Expected exit code: $expected_exit, got: $exit_code"
        ((TESTS_FAILED++))
        return 1
    fi
}

# Run a test expecting failure (non-zero exit)
# Usage: run_expect_fail "command"
run_expect_fail() {
    local cmd="$1"
    local exit_code=0

    eval "$cmd" >/dev/null 2>&1 || exit_code=$?

    if [[ $exit_code -ne 0 ]]; then
        echo -e "  ${GREEN}✓ PASS${NC}: $cmd (correctly failed with exit $exit_code)"
        ((TESTS_PASSED++))
        return 0
    else
        echo -e "  ${RED}✗ FAIL${NC}: $cmd (should have failed but succeeded)"
        ((TESTS_FAILED++))
        return 1
    fi
}

# Skip a test with reason
# Usage: skip_test "reason"
skip_test() {
    echo -e "  ${YELLOW}⊘ SKIP${NC}: $1"
    ((TESTS_SKIPPED++))
}

# Run command and capture output for inspection
# Usage: output=$(capture "command")
capture() {
    eval "$1" 2>&1
}

# Check if a command exists
has_command() {
    command -v "$1" >/dev/null 2>&1
}

# Print final test results
report_results() {
    local total=$((TESTS_PASSED + TESTS_FAILED + TESTS_SKIPPED))

    echo ""
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${BLUE}  Test Results: $CURRENT_SECTION${NC}"
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "  ${GREEN}Passed:${NC}  $TESTS_PASSED"
    echo -e "  ${RED}Failed:${NC}  $TESTS_FAILED"
    echo -e "  ${YELLOW}Skipped:${NC} $TESTS_SKIPPED"
    echo -e "  Total:   $total"
    echo ""

    if [[ $TESTS_FAILED -eq 0 ]]; then
        echo -e "${GREEN}All tests passed!${NC}"
        return 0
    else
        echo -e "${RED}Some tests failed!${NC}"
        return 1
    fi
}

# Export functions for use in test scripts
export -f section test_case run_test run_test_exit run_expect_fail skip_test capture has_command report_results
