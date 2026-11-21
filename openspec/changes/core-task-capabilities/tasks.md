## 1. Specification

- [ ] 1.1 Define configurable status requirements, transition validation, disabled-status behavior, and the unresolved outcome mapping.
- [ ] 1.2 Define stable task identity and no-physical-deletion requirements.
- [ ] 1.3 Define dependency relationships, cycle detection, and dependency-state exposure.
- [ ] 1.4 Define immutable operation history, version creation, approval baseline, and comparison requirements.
- [ ] 1.5 Define exclusive work-session locking, read exclusion, heartbeat, release, and recovery requirements.
- [ ] 1.6 Define atomic task/version/history persistence and migration integrity requirements.
- [ ] 1.7 Define versioned JSON contract and machine-readable error requirements.
- [ ] 1.8 Review all delta specs against PRODUCT.md and the architecture decision register.

## 2. Technical research and decisions

- [ ] 2.1 Compare candidate storage engines against transaction, concurrency, portability, backup, performance, and maintenance requirements.
- [ ] 2.2 Evaluate cross-platform lock and lease/recovery mechanisms.
- [ ] 2.3 Define status configuration semantics and completion/cancellation representation.
- [ ] 2.4 Define actor identity, history event semantics, version snapshots, and diff behavior.
- [ ] 2.5 Define JSON versioning, envelopes, error codes, and exit-code mapping.
- [ ] 2.6 Define migration validation, conflict handling, rollback, and recovery.
- [ ] 2.7 Record decisions and update the architecture decision register only after evidence-based review.

## 3. Verification

- [ ] 3.1 Validate the OpenSpec change with the OpenSpec CLI.
- [ ] 3.2 Check that every normative requirement has observable scenarios.
- [ ] 3.3 Check that no future capability is represented as implemented current behavior.
- [ ] 3.4 Review Windows and Unix-like implications.
