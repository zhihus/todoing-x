## ADDED Requirements

### Requirement: Stable task identifiers

The system MUST assign each task a stable identifier that is unique within its project store and MUST NOT reuse an identifier after the task is completed, cancelled, or otherwise removed from the active workflow.

#### Scenario: Create a task after an earlier task leaves active work

- **WHEN** a new task is created after another task has completed or cancelled
- **THEN** the new task receives a different identifier
- **AND** references to the earlier task remain unambiguous

### Requirement: Prohibit physical task deletion

The supported Todoing interface MUST NOT physically delete task records. Task completion and cancellation MUST NOT cause task data to be physically deleted.

#### Scenario: Request physical deletion

- **WHEN** a client requests deletion of a task
- **THEN** Todoing rejects the request
- **AND** the task record and its relationships remain intact
