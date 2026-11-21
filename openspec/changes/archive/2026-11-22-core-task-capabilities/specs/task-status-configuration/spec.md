## ADDED Requirements

### Requirement: User-configurable statuses

The system MUST allow each project to define the task statuses available in that project's task store. Todoing MUST NOT require a fixed built-in workflow or impose Manifest-specific status names.

#### Scenario: Use a project-defined status set

- **WHEN** a project provides a valid status configuration
- **THEN** Todoing uses that configuration as the source of available statuses
- **AND** does not add or impose consumer-specific statuses

### Requirement: Validate status transitions

The system MUST allow the project configuration to represent permitted status transitions and MUST reject a requested transition that is not permitted by the active configuration.

#### Scenario: Reject a disallowed transition

- **WHEN** a client requests a status transition that is not permitted
- **THEN** the operation is rejected
- **AND** the task's committed state remains unchanged

### Requirement: Preserve historical disabled statuses

Disabling a status MUST prevent its use for new transitions without invalidating historical task records that already contain that status.

#### Scenario: Read a task with a disabled historical status

- **WHEN** a task's current or historical version contains a status that has since been disabled
- **THEN** the task and its history remain readable
- **AND** the disabled status cannot be selected for a new transition

### Requirement: Distinguish completion and cancellation outcomes

The task model MUST represent completion and cancellation as distinct nullable outcomes independently from the configurable workflow status. A configured status MAY map to a terminal outcome. The explicit cancellation operation MUST set the cancelled outcome without requiring a dedicated workflow status and MUST accept an optional reason. Reopening MUST clear the terminal outcome.

#### Scenario: Preserve distinct terminal outcomes

- **WHEN** a task reaches a completed outcome or a cancelled outcome
- **THEN** clients can distinguish which outcome occurred
- **AND** the system does not infer one outcome solely from an arbitrary project-defined status name

#### Scenario: Cancel without a dedicated workflow status

- **WHEN** a client cancels a task from its current workflow status
- **THEN** the task outcome becomes cancelled
- **AND** its workflow status remains unchanged
- **AND** a cancellation reason may be supplied but is not required

#### Scenario: Reopen a task with a terminal outcome

- **WHEN** a client explicitly reopens a completed or cancelled task
- **THEN** the task outcome is cleared
- **AND** any subsequent status change is validated against the configured transition rules

#### Scenario: Reject a status change on a terminal task

- **WHEN** a client attempts to change the workflow status of a task with a completed or cancelled outcome without reopening it
- **THEN** the operation is rejected
- **AND** the task state and version history remain unchanged
