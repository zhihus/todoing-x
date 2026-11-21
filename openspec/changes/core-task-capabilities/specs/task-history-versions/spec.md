## ADDED Requirements

### Requirement: Append-only operation history

The system MUST maintain an append-only operation history for task operations. Each history entry MUST capture the actor, timestamp, operation, previous value, new value, and outcome.

Existing history entries MUST NOT be edited or deleted. Corrections MUST be represented by subsequent entries.

#### Scenario: Record a successful task operation

- **WHEN** a task operation is successfully committed
- **THEN** the operation history contains an entry describing the actor, timestamp, operation, previous value, new value, and successful outcome

### Requirement: Create a version for every successful save

Every successful task save MUST create a new immutable task version. A failed or rejected save MUST NOT create a successful version.

#### Scenario: Save a task

- **WHEN** a task mutation is successfully committed
- **THEN** a new version is created for that task
- **AND** the previous versions remain unchanged

### Requirement: Identify the user-approved baseline

The system MUST allow a task version to be identified as the version approved by the user. The baseline marker MUST refer to an existing immutable version.

#### Scenario: Approve a version

- **WHEN** the user approves a task version
- **THEN** that version is identifiable as the approved baseline
- **AND** later versions do not overwrite its content

### Requirement: Inspect and compare versions

The system MUST allow clients to inspect a task's versions and compare two versions of the same task.

#### Scenario: Compare task versions

- **WHEN** a client requests a comparison between two versions of one task
- **THEN** the system returns the differences between those versions
- **AND** leaves both versions unchanged
