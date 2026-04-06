## v0.8.1 Priorities

### 1. Rollback Validation Follow-Up

- [ ] Validate upgrade -> rollback (downgrade) flow on a real repo package with an available upgrade
- [ ] Validate upgrade -> rollback flow on a real AUR package with an available upgrade
- [ ] Validate missing pacman-cache artifact behavior during downgrade rollback
- [ ] Validate missing retained AUR artifact behavior during downgrade rollback
- [ ] Add any fixes discovered from those real-world rollback exercises

### 2. Rollback Hardening

- [ ] Add tests for post-rollback verification failures if practical
- [ ] Review rollback-attempt journaling after real usage and tighten any missing metadata
- [ ] Review rollback UX after real usage and improve error/help text where it is confusing

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


