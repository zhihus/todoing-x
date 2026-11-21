# Core task capabilities

## Why

The current Rust CLI is a compatibility prototype, not the target task-management system. Its hard-coded statuses, non-transactional task/index writes, and lack of dependencies, history, versions, and locks do not satisfy the accepted product baseline.

This change defines the target MVP behavior before technical research and architecture. It does not prescribe a storage engine, schema, configuration syntax, lock implementation, command syntax, or serialization format beyond the public JSON contract requirements.

## What changes

- Introduce user-configurable statuses and permitted transitions while keeping completion and cancellation outcomes semantically distinct.
- Add task dependencies, cycle detection, and dependency-state visibility.
- Add append-only operation history and immutable task versions, including a user-approved baseline and version comparison.
- Add exclusive task locks with owner identity, heartbeat, release, timeout, and abnormal-exit recovery.
- Require atomic persistence of task mutations, versions, history, and associated state.
- Establish stable task identity, prohibit physical deletion, and prevent identifier reuse.
- Define the CLI and versioned JSON interface as stable public contracts.
- Define a safe migration path from the current Markdown/YAML task records and JSON index.

## Out of scope

- Selecting SQLite or another storage engine; that follows technical comparison.
- Final database schema, file layout, status configuration syntax, lock protocol details, API envelopes, error-code catalogue, and exact CLI command syntax.
- Generic lifecycle events/hooks and Manifest-specific integration.
- Archive/restore implementation and archival policies; these are post-MVP capabilities. Physical deletion remains prohibited.
- Full import/export, backup automation, and graphical UI; these are post-MVP.
- Any Manifest, OpenSpec, Git workflow, or harness policy inside Todoing.

## Decisions and open questions

The requirements below derive from the accepted product baseline and decision register. SQLite remains preliminary. The following details remain open for technical design: the status configuration format; the relationship between configurable statuses and completion/cancellation outcomes; whether a cancelled prerequisite satisfies a dependency; lock owner identity and lease semantics; version snapshot and diff representation; JSON envelopes and error-code mapping; and migration cutover/rollback mechanics.

## Delivery boundary

This change establishes target capability requirements only. Technical research and architecture design are separate subsequent work. No implementation task is authorized by this proposal.
