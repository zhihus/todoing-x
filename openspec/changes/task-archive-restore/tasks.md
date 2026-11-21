## 1. Specification and design review

- [ ] 1.1 Review archive semantics against PRODUCT.md and the architecture decision register.
- [ ] 1.2 Confirm archive visibility, read access, and mutation restrictions.
- [ ] 1.3 Validate CLI/API operation names and request/response schemas.
- [ ] 1.4 Review lock, dependency, version, and migration interactions.

## 2. Test-first implementation

- [ ] 2.1 Add schema migration tests for archive metadata defaults and rollback.
- [ ] 2.2 Add repository tests for archive, restore, idempotency, and atomic version/history behavior.
- [ ] 2.3 Add tests for active/archive list filtering and direct reads.
- [ ] 2.4 Add tests for dependency retention and archived dependency state.
- [ ] 2.5 Add tests for archived-task mutation rejection and lock conflicts.
- [ ] 2.6 Add CLI and JSON API v1 contract tests, including stable error/exit behavior.
- [ ] 2.7 Add concurrency and failure-injection tests for archive/restore races.

## 3. Implementation

- [ ] 3.1 Add the versioned SQLite schema migration for archive metadata.
- [ ] 3.2 Implement transactional archive/restore repository operations.
- [ ] 3.3 Include archive metadata in task records and immutable version snapshots.
- [ ] 3.4 Enforce read-only behavior for archived tasks until restore.
- [ ] 3.5 Add active/archive list filtering and dependency archive-state exposure.
- [ ] 3.6 Add CLI archive/restore commands and archive listing option.
- [ ] 3.7 Add JSON API v1 archive/restore operations and schemas.
- [ ] 3.8 Update current-behavior OpenSpec specifications and README after verification.

## 4. Verification

- [ ] 4.1 Run cargo test and OpenSpec validation.
- [ ] 4.2 Verify Windows and Ubuntu CI.
- [ ] 4.3 Review migration compatibility and snapshot behavior.
- [ ] 4.4 Archive the completed OpenSpec change after all tasks are verified.
