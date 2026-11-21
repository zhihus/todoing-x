# Task status configuration

## Purpose

Describe project-configurable workflow statuses and the separation between workflow status and terminal task outcome.

## Requirements

### Requirement: User-configurable statuses

Each SQLite project MUST be able to define its task statuses and permitted transitions in `.todoing/config.toml`. Todoing MUST NOT impose Manifest-specific status names.

#### Scenario: Use a project-defined status set

- **WHEN** a project provides a valid status configuration
- **THEN** Todoing uses that configuration as the source of available statuses
- **AND** does not impose consumer-specific statuses

### Requirement: Validate status transitions

The system MUST reject a requested transition that is not permitted by the active project configuration. Disabled statuses MUST NOT be used for new transitions.

#### Scenario: Reject a disallowed transition

- **WHEN** a client requests a transition not permitted by configuration
- **THEN** the operation is rejected
- **AND** committed task state remains unchanged

### Requirement: Preserve historical disabled statuses

Disabling a status MUST NOT invalidate historical task records or immutable versions that already contain that status.

#### Scenario: Read a task with a disabled historical status

- **WHEN** a task or version contains a status that has since been disabled
- **THEN** the historical record remains readable
- **AND** the disabled status cannot be selected for a new transition

### Requirement: Distinguish workflow status from terminal outcome

The task model MUST represent `completed` and `cancelled` as distinct nullable outcomes independently from workflow status. A configured status MAY map to a terminal outcome. Explicit cancellation MUST NOT require a dedicated workflow status and MUST accept an optional reason. Reopening MUST clear the terminal outcome.

#### Scenario: Preserve distinct terminal outcomes

- **WHEN** a task reaches a completed or cancelled outcome
- **THEN** clients can distinguish which outcome occurred
- **AND** the system does not infer an outcome solely from an arbitrary status name

#### Scenario: Cancel without a dedicated workflow status

- **WHEN** a client cancels a task from its current workflow status
- **THEN** its outcome becomes cancelled
- **AND** workflow status remains unchanged

#### Scenario: Reject a status change on a terminal task

- **WHEN** a client attempts to change the workflow status of a terminal task without reopening it
- **THEN** the operation is rejected
- **AND** task state and version history remain unchanged
