## 1. Specification

- [x] 1.1 Define configurable status requirements, transition validation, disabled-status behavior, and the unresolved outcome mapping.
- [x] 1.2 Define stable task identity and no-physical-deletion requirements.
- [x] 1.3 Define dependency relationships, cycle detection, and dependency-state exposure.
- [x] 1.4 Define immutable operation history, version creation, approval baseline, and comparison requirements.
- [x] 1.5 Define exclusive work-session locking, read exclusion, heartbeat, release, and recovery requirements.
- [x] 1.6 Define atomic task/version/history persistence and migration integrity requirements.
- [x] 1.7 Define versioned JSON contract and machine-readable error requirements.
- [x] 1.8 Review all delta specs against PRODUCT.md and the architecture decision register.

## 2. Technical research and decisions

- [x] 2.1 Compare candidate storage engines against transaction, concurrency, portability, backup, performance, and maintenance requirements.
- [x] 2.2 Evaluate cross-platform lock and lease/recovery mechanisms.
- [x] 2.3 Define status configuration semantics and completion/cancellation representation.
- [x] 2.4 Define actor identity, history event semantics, version snapshots, and diff behavior.
- [x] 2.5 Define JSON versioning, envelopes, error codes, and exit-code mapping.
- [x] 2.6 Define migration validation, conflict handling, rollback, and recovery.
- [x] 2.7 Record decisions and update the architecture decision register only after evidence-based review.

## 3. Verification

- [x] 3.1 Validate the OpenSpec change with the OpenSpec CLI.
- [x] 3.2 Check that every normative requirement has observable scenarios.
- [x] 3.3 Check that no future capability is represented as implemented current behavior.
- [x] 3.4 Review Windows and Unix-like implications.


## 4. Implementation

- [ ] 4.1 Add domain types, actor context, error taxonomy, and TOML configuration parser with tests.
- [ ] 4.2 Add SQLite connection factory, schema migrations, constraints, immutable-table triggers, and integrity checks.
- [ ] 4.3 Implement transactional task repository, stable ID allocation, and no-physical-delete behavior.
- [ ] 4.4 Implement immutable versions, approved baselines, and append-only successful/rejected operation history.
- [ ] 4.5 Implement configurable statuses, completion/cancellation/reopen outcomes, and dependency cycle validation.
- [ ] 4.6 Implement exclusive task locks, read exclusion, lease heartbeat, recovery grace, fencing, and stale-owner rejection.
- [ ] 4.7 Implement human CLI compatibility and JSON API v1 request/response schemas, errors, and exit codes.
- [ ] 4.8 Implement non-destructive legacy migration and verified Online Backup API snapshots.
- [ ] 4.9 Complete cross-platform failure/concurrency/API/migration tests and update current-behavior specs only after verified implementation.
