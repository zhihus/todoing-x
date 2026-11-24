# Task final commit reference

## ADDED Requirements

### Requirement: Set one explicit final commit marker

The system MUST allow a task to have one explicit final Git commit SHA separate
from its ordinary commit references. Todoing MUST resolve the supplied revision
to a full SHA before storing it.

#### Scenario: Set the final commit marker

- **WHEN** a lock owner sets a valid final Git revision for a task without a marker
- **THEN** Todoing stores the full SHA as the final commit marker
- **AND** creates a task version and operation-history event atomically

### Requirement: Make final commit assignment idempotent

Repeating the same final SHA MUST be a no-op. A different SHA MAY replace the
current marker only after the task has been explicitly reopened. The previous
value MUST remain available in immutable task versions and operation history.
The marker MUST NOT be cleared.

#### Scenario: Repeat the same final SHA

- **WHEN** a client sets the already assigned final SHA again
- **THEN** the current task is returned with no_op=true
- **AND** no new version is created

#### Scenario: Update the final SHA after reopening

- **WHEN** a lock owner reopens a terminal task and assigns a different final SHA
- **THEN** the current marker is updated
- **AND** the previous marker remains in the prior immutable task version and history

#### Scenario: Reject final SHA mutation on a terminal task

- **WHEN** a client attempts to change the final SHA without reopening the task
- **THEN** the operation is rejected with task_terminal
- **AND** the existing marker remains unchanged
