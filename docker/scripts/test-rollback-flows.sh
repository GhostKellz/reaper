#!/bin/bash
# Real-flow transaction/rollback validation.
#
# Exercises genuine install -> rollback flows plus the missing-artifact
# classification paths that the rollback roadmap calls out:
#   1. Repo package install -> rollback (removal), with verification.
#   2. AUR package install  -> rollback (removal) (best-effort; skips if the
#      AUR build is unavailable in the sandbox).
#   3. Dry-run / apply parity: an artifact discoverable only via the pacman
#      cache fallback must NOT be reported "unavailable" by the preview.
#   4. Missing pacman-cache artifact during a downgrade rollback -> unavailable.
#   5. Missing retained AUR artifact during a downgrade rollback -> unavailable.
#
# Non-interactive: transaction IDs are read straight from the journal JSON.
source "$(dirname "$0")/common.sh"

section "Rollback Flow Validation"

TX_DIR="$HOME/.local/share/reap/history/transactions"

if ! has_command jq; then
    echo "jq is required for this harness" >&2
    skip_test "jq not installed"
    report_results
    exit 0
fi

# Newest transaction id whose affected_packages includes $1.
latest_txid_for() {
    local pkg="$1" newest="" newest_time=0 f mt id
    for f in "$TX_DIR"/*.json; do
        [ -e "$f" ] || continue
        if jq -e --arg p "$pkg" \
            '.affected_packages[]? | select(.name == $p)' "$f" >/dev/null 2>&1; then
            mt=$(stat -c %Y "$f")
            if [ "$mt" -ge "$newest_time" ]; then
                newest_time="$mt"
                id=$(jq -r '.id' "$f")
                newest="$id"
            fi
        fi
    done
    echo "$newest"
}

# Rewrite one field-set of the first affected package in a record file.
# Usage: mutate_record <txid> <jq-filter>
mutate_record() {
    local txid="$1" filter="$2" f="$TX_DIR/$1.json"
    jq "$filter" "$f" >"$f.tmp" && mv "$f.tmp" "$f"
}

# ---------------------------------------------------------------------------
# Scenario 1: repo package install -> rollback removal
# ---------------------------------------------------------------------------
REPO_PKG="tree"

test_case "Repo install records a rollback transaction"
sudo pacman -Rns --noconfirm "$REPO_PKG" >/dev/null 2>&1 || true
reap install "$REPO_PKG" </dev/null >/tmp/rb_repo_install.log 2>&1 || true
if pacman -Q "$REPO_PKG" >/dev/null 2>&1; then
    REPO_TXID=$(latest_txid_for "$REPO_PKG")
    if [ -n "$REPO_TXID" ]; then
        echo -e "  ${GREEN}✓ PASS${NC}: installed $REPO_PKG, recorded $REPO_TXID"
        ((TESTS_PASSED++))
    else
        echo -e "  ${RED}✗ FAIL${NC}: $REPO_PKG installed but no transaction recorded"
        ((TESTS_FAILED++))
    fi
else
    echo -e "  ${RED}✗ FAIL${NC}: $REPO_PKG did not install (see /tmp/rb_repo_install.log)"
    ((TESTS_FAILED++))
    REPO_TXID=""
fi

test_case "Repo rollback removes the freshly installed package"
if [ -n "$REPO_TXID" ]; then
    reap rollback apply "$REPO_TXID" --yes </dev/null >/tmp/rb_repo_apply.log 2>&1 || true
    if ! pacman -Q "$REPO_PKG" >/dev/null 2>&1; then
        echo -e "  ${GREEN}✓ PASS${NC}: $REPO_PKG removed by rollback"
        ((TESTS_PASSED++))
    else
        echo -e "  ${RED}✗ FAIL${NC}: $REPO_PKG still installed after rollback"
        cat /tmp/rb_repo_apply.log
        ((TESTS_FAILED++))
    fi
else
    skip_test "no repo transaction id to roll back"
fi

# ---------------------------------------------------------------------------
# Scenario 2: AUR package install -> rollback removal (best-effort)
# ---------------------------------------------------------------------------
# 'downgrade' is a small, dependency-light, pure-shell AUR package.
AUR_PKG="downgrade"

test_case "AUR install records a rollback transaction"
AUR_TXID=""
reap install "$AUR_PKG" </dev/null >/tmp/rb_aur_install.log 2>&1 || true
if pacman -Q "$AUR_PKG" >/dev/null 2>&1; then
    AUR_TXID=$(latest_txid_for "$AUR_PKG")
    if [ -n "$AUR_TXID" ]; then
        echo -e "  ${GREEN}✓ PASS${NC}: installed $AUR_PKG, recorded $AUR_TXID"
        ((TESTS_PASSED++))
    else
        echo -e "  ${RED}✗ FAIL${NC}: $AUR_PKG installed but no transaction recorded"
        ((TESTS_FAILED++))
    fi
else
    skip_test "AUR build unavailable in sandbox (see /tmp/rb_aur_install.log)"
fi

test_case "AUR rollback removes the freshly installed package"
if [ -n "$AUR_TXID" ]; then
    reap rollback apply "$AUR_TXID" --yes </dev/null >/tmp/rb_aur_apply.log 2>&1 || true
    if ! pacman -Q "$AUR_PKG" >/dev/null 2>&1; then
        echo -e "  ${GREEN}✓ PASS${NC}: $AUR_PKG removed by rollback"
        ((TESTS_PASSED++))
    else
        echo -e "  ${RED}✗ FAIL${NC}: $AUR_PKG still installed after rollback"
        cat /tmp/rb_aur_apply.log
        ((TESTS_FAILED++))
    fi
else
    skip_test "no AUR transaction id to roll back"
fi

# ---------------------------------------------------------------------------
# Build a real upgrade-shaped record we can mutate for the artifact scenarios.
# Install tree via pacman so its artifact is genuinely present in the pacman
# cache, then record an install transaction through reap.
# ---------------------------------------------------------------------------
sudo pacman -Rns --noconfirm "$REPO_PKG" >/dev/null 2>&1 || true
reap install "$REPO_PKG" </dev/null >/tmp/rb_repo_install2.log 2>&1 || true
MUT_TXID=$(latest_txid_for "$REPO_PKG")
REPO_VER=$(pacman -Q "$REPO_PKG" 2>/dev/null | awk '{print $2}')

# ---------------------------------------------------------------------------
# Scenario 3: dry-run / apply parity via pacman-cache fallback.
# previous_artifact is null but the version IS in the pacman cache, so the
# preview must locate it (it must not report the package "unavailable").
# This is the regression guard for the dry-run/apply divergence fix.
# ---------------------------------------------------------------------------
test_case "Dry-run finds cache-only artifact (parity with apply)"
if [ -n "$MUT_TXID" ] && [ -n "$REPO_VER" ]; then
    mutate_record "$MUT_TXID" \
        ".affected_packages[0].change_type = \"Upgrade\"
         | .affected_packages[0].source = \"Pacman\"
         | .affected_packages[0].previous_version = \"$REPO_VER\"
         | .affected_packages[0].new_version = \"$REPO_VER\"
         | .affected_packages[0].previous_artifact = null
         | .operation = \"Upgrade\""
    out=$(reap rollback dry-run "$MUT_TXID" </dev/null 2>&1)
    if echo "$out" | grep -q "Unavailable:  0"; then
        echo -e "  ${GREEN}✓ PASS${NC}: cache-only artifact classified as rollbackable"
        ((TESTS_PASSED++))
    else
        echo -e "  ${RED}✗ FAIL${NC}: preview marked a cache-discoverable artifact unavailable"
        echo "$out"
        ((TESTS_FAILED++))
    fi
else
    skip_test "could not stage a mutable repo transaction"
fi
sudo pacman -Rns --noconfirm "$REPO_PKG" >/dev/null 2>&1 || true

# ---------------------------------------------------------------------------
# Scenario 4: missing pacman-cache artifact -> unavailable.
# previous_artifact points at a non-existent file and the version is not in
# any cache, so the rollback must be reported unavailable.
# ---------------------------------------------------------------------------
test_case "Missing pacman-cache artifact is reported unavailable"
reap install "$REPO_PKG" </dev/null >/tmp/rb_repo_install3.log 2>&1 || true
MISS_TXID=$(latest_txid_for "$REPO_PKG")
if [ -n "$MISS_TXID" ]; then
    mutate_record "$MISS_TXID" \
        '.affected_packages[0].change_type = "Upgrade"
         | .affected_packages[0].source = "Pacman"
         | .affected_packages[0].previous_version = "0.0.0-1"
         | .affected_packages[0].previous_artifact = "/var/cache/pacman/pkg/tree-0.0.0-1-x86_64.pkg.tar.zst"
         | .operation = "Upgrade"'
    out=$(reap rollback dry-run "$MISS_TXID" </dev/null 2>&1)
    if echo "$out" | grep -q "Unavailable:  1"; then
        echo -e "  ${GREEN}✓ PASS${NC}: missing repo artifact correctly unavailable"
        ((TESTS_PASSED++))
    else
        echo -e "  ${RED}✗ FAIL${NC}: missing repo artifact not reported unavailable"
        echo "$out"
        ((TESTS_FAILED++))
    fi
else
    skip_test "could not stage a repo transaction for missing-artifact test"
fi
sudo pacman -Rns --noconfirm "$REPO_PKG" >/dev/null 2>&1 || true

# ---------------------------------------------------------------------------
# Scenario 5: missing retained AUR artifact -> unavailable.
# Same shape as scenario 4 but the change is an AUR-sourced downgrade whose
# retained artifact no longer exists.
# ---------------------------------------------------------------------------
test_case "Missing retained AUR artifact is reported unavailable"
reap install "$REPO_PKG" </dev/null >/tmp/rb_repo_install4.log 2>&1 || true
AURMISS_TXID=$(latest_txid_for "$REPO_PKG")
if [ -n "$AURMISS_TXID" ]; then
    mutate_record "$AURMISS_TXID" \
        ".affected_packages[0].change_type = \"Upgrade\"
         | .affected_packages[0].source = \"Aur\"
         | .affected_packages[0].previous_version = \"0.0.0-1\"
         | .affected_packages[0].previous_artifact = \"$HOME/.local/share/reap/artifacts/aur/tree-0.0.0-1-x86_64.pkg.tar.zst\"
         | .operation = \"Upgrade\""
    out=$(reap rollback dry-run "$AURMISS_TXID" </dev/null 2>&1)
    if echo "$out" | grep -q "Unavailable:  1"; then
        echo -e "  ${GREEN}✓ PASS${NC}: missing retained AUR artifact correctly unavailable"
        ((TESTS_PASSED++))
    else
        echo -e "  ${RED}✗ FAIL${NC}: missing AUR artifact not reported unavailable"
        echo "$out"
        ((TESTS_FAILED++))
    fi
else
    skip_test "could not stage a repo transaction for AUR missing-artifact test"
fi
sudo pacman -Rns --noconfirm "$REPO_PKG" >/dev/null 2>&1 || true

report_results
