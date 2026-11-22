## ADDED Requirements

### Requirement: Archive tasks without changing lifecycle status

The system MUST provide an explicit archive operation that removes a task from the active task list without changing its workflow status or terminal outcome. Archiving MUST NOT physically delete the task or its history, versions, dependencies, labels, or commit references.

#### Scenario: Archive an active task

- **WHEN** a client archives an active task
- **THEN** the task receives archive metadata
- **AND** it is omitted from the default active task list
- **AND** its workflow status and terminal outcome remain unchanged

#### Scenario: Archive a completed or cancelled task

- **WHEN** a client archives a task with a completed or cancelled outcome
- **THEN** the task becomes archived
- **AND** its terminal outcome remains unchanged

### Requirement: Inspect archived tasks

The system MUST provide an archive view that lists archived tasks. A client MUST be able to inspect an archived task by its stable ID when it is not excluded by a task lock.

#### Scenario: List archived tasks

- **WHEN** a client requests the archive view
- **THEN** only archived tasks are returned
- **AND** the default active task list continues to omit them

#### Scenario: Read an archived task

- **WHEN** a client reads an archived task by ID and no other client holds its lock
- **THEN** the task and its retained metadata remain available

### Requirement: Restore archived tasks

The system MUST provide an explicit restore operation. Restoration MUST clear archive metadata and MUST NOT change the task's workflow status, terminal outcome, identity, dependencies, or history.

#### Scenario: Restore a task

- **WHEN** a client restores an archived task
- **THEN** the task returns to the active task list
- **AND** its workflow status and terminal outcome are unchanged

### Requirement: Preserve dependencies when archiving

Archiving MUST NOT cascade to related tasks or remove dependency relationships. Dependency inspection MUST identify an archived dependency without hiding or deleting the relationship.

#### Scenario: Retain an archived dependency

- **WHEN** a task depends on a task that is archived
- **THEN** the dependency relationship remains
- **AND** dependency state identifies the referenced task as archived

### Requirement: Restrict mutation of archived tasks

Archived tasks MUST be read-only until restored. Ordinary task mutations and new lock acquisition MUST be rejected with a stable `task_archived` error. Archive and restore operations MUST respect existing task lock ownership rules.

#### Scenario: Reject an ordinary mutation

- **WHEN** a client attempts to update an archived task without restoring it
- **THEN** the operation is rejected with `task_archived`
- **AND** the task state and version history remain unchanged

#### Scenario: Reject archive while another client owns the lock

- **WHEN** a client attempts to archive or restore a task locked by another client
- **THEN** the operation is rejected with the existing lock-conflict error
- **AND** archive state remains unchanged

### Requirement: Version and record archive operations

A successful archive or restore operation that changes archive state MUST create an immutable task version and an append-only history event atomically. Repeating an operation that already matches the current state MUST be an accepted no-op and MUST NOT create a task version.

#### Scenario: Record an archive operation

- **WHEN** a task is archived successfully
- **THEN** the new version contains archive metadata
- **AND** history records actor, timestamp, and `task.archive`

#### Scenario: Record a restore operation

- **WHEN** an archived task is restored successfully
- **THEN** the new version contains cleared archive metadata
- **AND** history records actor, timestamp, and `task.restore`

#### Scenario: Repeat an archive operation

- **WHEN** a client archives a task that is already archived
- **THEN** the operation succeeds with `no_op=true`
- **AND** history records the accepted no-op
- **AND** no new version is created
