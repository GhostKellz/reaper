#!/bin/bash
# Test basic CLI functionality
source "$(dirname "$0")/common.sh"

section "CLI Basic Tests"

test_case "Version output"
run_test "reap --version" "reap"

test_case "Help output"
run_test "reap --help" "Reaper"

test_case "Help shows install command"
run_test "reap --help" "install"

test_case "Help shows search command"
run_test "reap --help" "search"

test_case "Doctor check runs"
run_test "reap doctor" ""

test_case "Invalid command fails"
run_expect_fail "reap invalidcommand12345"

test_case "Pacman-style -h flag"
run_test "reap -h" "Reaper"

test_case "Completions command exists"
run_test "reap completion bash" ""

report_results
