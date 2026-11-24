# Task history and versions

## Purpose

Describe immutable task versions, append-only operation history, and explicit user-approved baselines in the SQLite backend.

## Requirements

### Requirement: Append-only operation history

The system MUST maintain append-only operation history for task operations. Each event MUST identify the actor, timestamp, operation, outcome, and applicable previous/requested values or details. Existing events MUST NOT be edited or deleted; corrections MUST be represented by later events.

#### Scenario: Record a successful task operation

- **WHEN** a task operation is successfully committed
- **THEN** history contains an event describing the actor, timestamp, operation, and successful outcome

#### Scenario: Record a rejected task operation

- **WHEN** a task operation reaches the operation layer and is rejected by a business or concurrency rule
- **THEN** history records the attempted operation and rejection outcome
- **AND** no successful task version is created
- **AND** committed task state remains unchanged

#### Scenario: Record an accepted no-op

- **WHEN** a valid task operation is accepted but does not change task state
- **THEN** history records a successful no-op event
- **AND** no new task version is created

### Requirement: Create immutable task versions

Every successful task-state save MUST create a new immutable task version in the same transaction as the task mutation and successful history event. Failed or rejected saves MUST NOT create successful versions.

#### Scenario: Save a task

- **WHEN** a task mutation is successfully committed
- **THEN** a new version is created
- **AND** all previous versions remain unchanged

### Requirement: Inspect and compare versions

The system MUST allow clients to list a task's versions, retrieve an individual version, and compare two versions of the same task.

#### Scenario: Compare task versions

- **WHEN** a client requests a comparison between two versions of one task
- **THEN** the system returns deterministic field-level differences
- **AND** leaves both versions unchanged

### Requirement: Identify the user-approved baseline

The system MUST allow an existing task version to be identified as the user-approved baseline. The baseline MUST reference an immutable version belonging to the same task. A later save MUST NOT change the baseline implicitly.

#### Scenario: Approve a version

- **WHEN** a user explicitly approves a task version
- **THEN** that version becomes the identifiable approved baseline
- **AND** later versions do not overwrite its content

#### Scenario: Do not approve a baseline implicitly

- **WHEN** an agent saves a task or creates a later version without an explicit approval operation
- **THEN** the user-approved baseline remains unchanged

### Requirement: Protect history and versions under task locks

History, version, and baseline reads MUST obey task lock read exclusion.

#### Scenario: Read history for a locked task

- **WHEN** a client other than the lock owner requests a task's history, versions, or baseline
- **THEN** the request is rejected without disclosing protected task data

### Requirement: Preserve extension metadata in immutable task versions

Task versions MUST snapshot active external references, `revision_count`, and `final_commit_sha`. Review records MUST be append-only and linked to an existing immutable version of the same task. A newly accepted `changes_requested` Review record MUST increment `revision_count` exactly once; an approved record or idempotent retry MUST NOT increment it.

#### Scenario: Compare versions containing extension metadata

- **WHEN** a client compares versions across an external-reference, Review counter, or final-commit change
- **THEN** the comparison reports the corresponding deterministic field-level differences
- **AND** historical snapshots remain unchanged

#### Scenario: Reject a conflicting Review retry

- **WHEN** a client reuses a Review ID with a different canonical payload
- **THEN** the operation is rejected with `review_id_conflict`
- **AND** no Review record, version, history event, or counter change is committed
