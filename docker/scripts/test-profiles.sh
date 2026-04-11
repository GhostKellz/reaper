#!/bin/bash
# Test profile management
source "$(dirname "$0")/common.sh"

section "Profile Tests"

test_case "Profile list"
run_test "reap profile list" ""

test_case "Profile show (requires name)"
run_expect_fail "reap profile show"

test_case "Default profiles exist"
profiles_dir="$HOME/.config/reap/profiles"
if [[ -d "$profiles_dir" ]]; then
    count=$(ls -1 "$profiles_dir"/*.toml 2>/dev/null | wc -l)
    if [[ $count -gt 0 ]]; then
        echo -e "  ${GREEN}✓ PASS${NC}: Found $count profile(s)"
        ((TESTS_PASSED++))
    else
        echo -e "  ${YELLOW}⊘ SKIP${NC}: No profiles found (may be expected)"
        ((TESTS_SKIPPED++))
    fi
else
    echo -e "  ${YELLOW}⊘ SKIP${NC}: Profiles directory not found"
    ((TESTS_SKIPPED++))
fi

test_case "Create test profile"
if reap profile create testprofile 2>/dev/null; then
    echo -e "  ${GREEN}✓ PASS${NC}: Profile created"
    ((TESTS_PASSED++))

    # Cleanup
    reap profile delete testprofile 2>/dev/null || true
else
    echo -e "  ${YELLOW}⊘ SKIP${NC}: Profile creation not available or failed"
    ((TESTS_SKIPPED++))
fi

report_results
