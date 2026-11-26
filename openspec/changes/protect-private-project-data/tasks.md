## Implementation

- [x] Add shared Git safety guard for private project data.
- [x] Run the guard before initialization creates `.todoing/`.
- [x] Run the guard before migration reads or writes private data.
- [x] Add regression tests for ignore setup and tracked-data refusal.
- [x] Correct README and product documentation.
- [x] Audit the current repository tree for private project data.

## Verification

- [ ] Run formatting and the full Rust test suite on Windows and Linux.
- [ ] Verify GitHub CI passes after the change.
- [ ] Verify no `.todoing/`, database, migration report, or snapshot files are tracked.
