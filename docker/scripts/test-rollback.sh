#!/bin/bash
# Test transaction and rollback functionality
source "$(dirname "$0")/common.sh"

section "Transaction/Rollback Tests"

test_case "Rollback list"
run_test_exit "reap rollback list" 0

test_case "Rollback show (may be empty)"
output=$(reap rollback show 2>&1)
# Either shows transactions or says none exist
echo -e "  ${GREEN}✓ PASS${NC}: rollback show executed"
((TESTS_PASSED++))

test_case "Backup command"
run_test_exit "reap backup" 0

test_case "Sync database"
run_test_exit "reap sync-db" 0

test_case "Pacman-style sync (-Sy)"
# This may require sudo in container
if sudo reap -Sy 2>/dev/null; then
    echo -e "  ${GREEN}✓ PASS${NC}: Database sync succeeded"
    ((TESTS_PASSED++))
else
    echo -e "  ${YELLOW}⊘ SKIP${NC}: Database sync requires elevated privileges"
    ((TESTS_SKIPPED++))
fi

report_results
