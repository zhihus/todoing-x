# Task history and versions

## Requirements

### Requirement: Create a version for each successful save

Every successful task save that changes task state MUST create a new identifiable task version containing a complete snapshot of the resulting task state.

A rejected or failed mutation MUST NOT create a successful task version. A no-op request that does not change task state MUST NOT create a new version.

#### Scenario: Save a task

- **WHEN** a task mutation is successfully committed
- **THEN** the system creates a new version representing the resulting task state
- **AND** the version is associated with the task

#### Scenario: Reject a mutation

- **WHEN** a task mutation fails validation or persistence
- **THEN** no successful version is created for that mutation

### Requirement: Maintain append-only operation history

The system MUST maintain an append-only history of task operations.

Each history entry MUST record the actor, timestamp, operation, previous value, new value, and outcome. Existing history entries MUST be immutable; corrections MUST be represented by subsequent entries.

#### Scenario: Record a successful operation

- **WHEN** a task operation succeeds
- **THEN** the system appends a history entry describing the operation and its result

#### Scenario: Preserve prior history

- **WHEN** a later task operation changes task state
- **THEN** earlier history entries remain unchanged and available for inspection

### Requirement: Identify an approved baseline version

The system MUST allow a version to be identified as the user-approved baseline for a task.

The approval marker MUST refer to a specific task version and MUST remain distinguishable from ordinary version creation. Re-approval MUST update the current baseline reference without modifying prior versions; the approval change MUST be recorded in operation history.

#### Scenario: Approve a task version

- **WHEN** the user approves the current task version
- **THEN** the system records that exact version as the approved baseline

### Requirement: Inspect and compare versions

The system MUST allow clients to inspect task versions and compare two versions of the same task.

#### Scenario: Compare task versions

- **WHEN** a client requests a comparison between two existing versions of a task
- **THEN** the system returns the differences between their task states
