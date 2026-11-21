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
- [ ] Add failing tests for TOML schema v1, default statuses, unique status IDs, transition validation, disabled statuses, and retired historical statuses.
- [ ] Add failing tests for status-to-outcome mapping, clearing outcome on reopen, and preserving prior outcomes in history.
- [ ] Add failing tests for dependency validation and direct/indirect cycle rejection.
- [ ] Add failing tests for atomic task/version/history mutation and immutable history.
- [ ] Add failing tests for approved baseline selection and version comparison.
- [ ] Add failing multi-process tests for the shared/exclusive file-lock barrier, task lease ownership, strict read exclusion, heartbeat, release, and stale-lock recovery.
- [ ] Add failing tests proving list/filter omit tasks locked by another owner and dependency inspection reports locked dependencies as unavailable.
- [ ] Add failing contract tests for the api_version/ok/data-or-error JSON envelope, stable error codes, physical-delete rejection, and exit codes.
- [ ] Add failing migration tests for task IDs/content/Git refs/unknown frontmatter preservation, initial versions/history, malformed source data, target conflicts, and non-destructive behavior.
- [ ] Verify acceptance tests fail for the expected missing behavior before implementing the corresponding capability.

## Implementation

- [ ] Implement the approved task model and configurable lifecycle.
- [ ] Implement dependency operations and cycle detection.
- [ ] Implement transactional task mutation, version creation, and history append.
- [ ] Implement exclusive task locks and abandoned-lock recovery.
- [ ] Implement the approved CLI and versioned JSON contract.
- [ ] Implement and test migration from the current Markdown/YAML and JSON format.
- [ ] Verify the implementation on supported Windows and Unix-like environments.
