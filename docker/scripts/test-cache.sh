#!/bin/bash
# Test cache operations
source "$(dirname "$0")/common.sh"

section "Cache Tests"

test_case "Performance cache stats"
run_test_exit "reap perf cache-stats" 0

test_case "Cache warm"
run_test_exit "reap perf warm-cache" 0

test_case "Clean command"
run_test_exit "reap clean" 0

test_case "Pacman-style cache clean (-Sc)"
run_test_exit "reap -Sc" 0

report_results
