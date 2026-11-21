# Task history and versions

## ADDED Requirements

### Requirement: Append-only operation history

Todoing MUST maintain an append-only history of task operations. Each history entry MUST identify the actor, timestamp, operation, previous value, new value, and outcome where those fields apply.

Existing history entries MUST be immutable. Corrections MUST be represented by subsequent entries rather than edits to prior history.

#### Scenario: Record a task operation

- **WHEN** a client performs a task operation
- **THEN** Todoing appends a history entry describing the operation and its outcome
- **AND** does not modify earlier history entries

### Requirement: Create a version for every successful task save

Every successful task save MUST create a new immutable task version.

#### Scenario: Save a task change

- **WHEN** a task mutation is successfully committed
- **THEN** a new task version is created
- **AND** the version can be retrieved independently of the current task state

### Requirement: Identify the user-approved baseline version

Todoing MUST allow a task version to be identified as the version approved by the user.

#### Scenario: Approve a task version

- **WHEN** the user approves the current task version
- **THEN** that version is marked as the approved baseline
- **AND** the baseline remains identifiable after later task changes

### Requirement: Inspect and compare task versions

Todoing MUST allow clients to inspect task versions and compare two versions of the same task.

#### Scenario: Compare task versions

- **WHEN** a client requests a comparison between two existing versions of a task
- **THEN** Todoing returns their differences
- **AND** leaves both versions unchanged
