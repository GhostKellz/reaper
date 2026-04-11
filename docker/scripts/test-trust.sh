#!/bin/bash
# Test trust scoring system
source "$(dirname "$0")/common.sh"

section "Trust Scoring Tests"

test_case "Trust stats"
run_test_exit "reap trust stats" 0

test_case "Trust score for known package"
run_test "reap trust score neovim" ""

test_case "Trust scan"
run_test_exit "reap trust scan" 0

test_case "Trust update"
run_test_exit "reap trust update" 0

report_results
