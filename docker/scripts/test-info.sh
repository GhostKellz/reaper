#!/bin/bash
# Test package info commands
source "$(dirname "$0")/common.sh"

section "Package Info Tests"

test_case "Query installed packages (-Q)"
run_test "reap -Q" ""

test_case "Info for base package"
# This might fail in container if base isn't installed
if pacman -Q base >/dev/null 2>&1; then
    run_test "reap -Q base" "base"
else
    skip_test "base package not installed"
fi

test_case "Query upgradable packages (-Qu)"
# Just check it doesn't error
run_test_exit "reap -Qu" 0

test_case "Orphan packages check"
run_test_exit "reap orphan" 0

report_results
