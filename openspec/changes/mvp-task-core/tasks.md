# Tasks

## Requirements and design

- [ ] Review the capability deltas against the accepted Product Definition and decision register.
- [ ] Compare storage options against transaction, concurrency, locking, portability, recovery, backup, and maintenance requirements.
- [ ] Decide the task identity, status configuration, and transition-validation models.
- [ ] Define dependency representation and cycle-detection behavior.
- [ ] Define version snapshots, immutable history records, actor identity, and baseline approval semantics.
- [ ] Define the lock ownership, heartbeat, lease timeout, and abnormal-exit recovery protocol.
- [ ] Define the versioned JSON contract, error codes, and process exit-code mapping.
- [ ] Define the non-destructive migration procedure and integrity validation for existing task data.

## Acceptance tests — TEST FIRST

- [ ] Add failing tests for stable task IDs, no ID reuse, and prohibited physical deletion.
- [ ] Add failing tests for configurable statuses, permitted transitions, and retired historical statuses.
- [ ] Add failing tests for dependency validation and direct/indirect cycle rejection.
- [ ] Add failing tests for atomic task/version/history mutation and immutable history.
- [ ] Add failing tests for approved baseline selection and version comparison.
- [ ] Add failing multi-process tests for task locks, strict read exclusion, heartbeat, release, and stale-lock recovery.
- [ ] Add failing contract tests for JSON versioning, stable error codes, and exit codes.
- [ ] Add failing migration tests for preservation, malformed source data, and non-destructive behavior.
- [ ] Verify acceptance tests fail for the expected missing behavior before implementing the corresponding capability.

## Implementation

- [ ] Implement the approved task model and configurable lifecycle.
- [ ] Implement dependency operations and cycle detection.
- [ ] Implement transactional task mutation, version creation, and history append.
- [ ] Implement exclusive task locks and abandoned-lock recovery.
- [ ] Implement the approved CLI and versioned JSON contract.
- [ ] Implement and test migration from the current Markdown/YAML and JSON format.
- [ ] Verify the implementation on supported Windows and Unix-like environments.
