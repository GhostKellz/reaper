## v0.8.2 Priorities

### 1. Rollback Validation Follow-Up

Validated in the Arch container via `docker/scripts/test-rollback-flows.sh`.

- [x] Validate repo package install -> rollback (removal) flow with version verification
- [x] Validate AUR package install -> rollback (removal) flow (real `makepkg` build)
- [x] Validate downgrade-plan classification via the pacman-cache artifact fallback
- [x] Validate missing pacman-cache artifact behavior during downgrade rollback (reported unavailable)
- [x] Validate missing retained AUR artifact behavior during downgrade rollback (reported unavailable)
- [x] Fix discovered: `rollback dry-run` re-derived the plan from `previous_artifact.exists()`
      only, so it could disagree with `rollback apply` (which uses the pacman-cache fallback).
      The preview now drives off the authoritative `create_rollback_plan`; guarded by a unit test.
- [ ] Full two-version upgrade -> downgrade with a genuine prior version is gated on Arch Linux
      Archive support (see section 5) for deterministic historical artifacts.

### 2. Rollback Hardening

- [x] Add tests for post-rollback verification failures. Extracted the attempt
      classifier into `classify_rollback_attempt` and unit-tested it, including the
      case where every package op "succeeded" but verification failed -> recorded as
      `PartialSuccess`, never `Success`.
- [x] Fixed confusing rollback error text: a missing transaction printed the
      "Transaction not found" prefix twice; it now prints the error once plus a
      `reap rollback list` discovery hint.
- [ ] Review rollback-attempt journaling after real usage and tighten any missing metadata
- [ ] Review rollback UX further after real usage and improve remaining help text

### 3. Documentation Cleanup

- [ ] Finish the docs cleanup and final reorganization pass
- [ ] Reduce any remaining root-doc clutter and keep canonical docs under `docs/`
- [ ] Re-audit README, CONTRIBUTING, SECURITY, and command reference after the `v0.8.0` commit

### 4. Post-Release Polish

- [ ] Review release notes and changelog accuracy after the `v0.8.0` commit
- [ ] Re-check package metadata, PKGBUILD, and install instructions after release
- [ ] Fix any regressions or rough edges found immediately after the `v0.8.0` release

### 5. Optional Feature Work

- [ ] Optional Arch Linux Archive support for historical repo rollback
- [ ] Optional filesystem snapshot integration for true atomic rollback
  - btrfs
  - zfs
  - maybe LVM

### 6. Future Direction

- [ ] Re-evaluate broader multi-source follow-up only if it does not compromise the Arch-native core
- [ ] Decide whether the next major expansion is docs polish, rollback enhancements, or broader package-source work
