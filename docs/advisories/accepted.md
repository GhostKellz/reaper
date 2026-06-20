# Accepted Advisories

Security advisories that are knowingly accepted because they cannot currently be
removed, such as a vulnerable crate pulled only transitively with no upstream
fix available.

**There are currently no accepted advisories.** `cargo audit` passes without an
ignore list.

## Process for accepting a new advisory

1. Confirm the advisory cannot be cleared by `cargo update` or a dependency
   feature change.
2. If a CI advisory gate is added, add the `RUSTSEC-XXXX-XXXX` ID to that
   tool's explicit ignore list.
3. Add a row to the table below with the rationale and a review date.

| Advisory | Crate | Severity | Source chain | Rationale | Review date |
|----------|-------|----------|--------------|-----------|-------------|
| _(none)_ | | | | | |
