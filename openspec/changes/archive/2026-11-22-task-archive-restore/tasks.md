## 1. Specification and design review

- [x] 1.1 Review archive semantics against PRODUCT.md and the architecture decision register.
- [x] 1.2 Confirm archive visibility, read access, and mutation restrictions.
- [x] 1.3 Validate CLI/API operation names and request/response schemas.
- [x] 1.4 Review lock, dependency, version, and migration interactions.

## 2. Test-first implementation

- [x] 2.1 Add schema migration tests for archive metadata defaults and rollback.
- [x] 2.2 Add repository tests for archive, restore, idempotency, and atomic version/history behavior.
- [x] 2.3 Add tests for active/archive list filtering and direct reads.
- [x] 2.4 Add tests for dependency retention and archived dependency state.
- [x] 2.5 Add tests for archived-task mutation rejection and lock conflicts.
- [x] 2.6 Add CLI and JSON API v1 contract tests, including stable error/exit behavior.
- [x] 2.7 Add concurrency and failure-injection tests for archive/restore races.

## 3. Implementation

- [x] 3.1 Add the versioned SQLite schema migration for archive metadata.
- [x] 3.2 Implement transactional archive/restore repository operations.
- [x] 3.3 Include archive metadata in task records and immutable version snapshots.
- [x] 3.4 Enforce read-only behavior for archived tasks until restore.
- [x] 3.5 Add active/archive list filtering and dependency archive-state exposure.
- [x] 3.6 Add CLI archive/restore commands and archive listing option.
- [x] 3.7 Add JSON API v1 archive/restore operations and schemas.
- [x] 3.8 Update current-behavior OpenSpec specifications and README after verification.

## 4. Verification

- [x] 4.1 Run cargo test and OpenSpec validation.
- [x] 4.2 Verify Windows and Ubuntu CI.
- [x] 4.3 Review migration compatibility and snapshot behavior.
- [x] 4.4 Archive the completed OpenSpec change after all tasks are verified.
