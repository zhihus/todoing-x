# Task identity and lifecycle

## Requirements

### Requirement: Stable task identity

The system MUST assign each task a stable identifier that remains unchanged throughout the task's lifetime.

The system MUST NOT reuse an identifier after a task has been completed, cancelled, or otherwise removed from the active workflow. Physical task deletion MUST NOT be supported.

#### Scenario: Create a task

- **WHEN** a new task is created
- **THEN** the system assigns an identifier that is unique within the project task store
- **AND** that identifier remains associated with the task

#### Scenario: Retain identity after workflow completion

- **WHEN** a task reaches a completion or cancellation outcome
- **THEN** its identifier remains reserved
- **AND** the task record is retained

### Requirement: User-configurable workflow statuses

The system MUST allow the user to define the workflow statuses available in a project.

The configuration MUST allow users to control status identity, display ordering, and which statuses are available for new transitions. The exact configuration syntax and schema are an architecture decision.

The system MUST validate status values and permitted transitions against the active project configuration.

#### Scenario: Use a configured status

- **WHEN** a client creates or updates a task using an enabled configured status and a permitted transition
- **THEN** the system accepts the status change

#### Scenario: Reject an unavailable status or transition

- **WHEN** a client requests a disabled or undefined status, or a transition not permitted by the configuration
- **THEN** the system rejects the mutation
- **AND** leaves the task unchanged

### Requirement: Preserve historical statuses

Disabling or removing a status from the active workflow configuration MUST NOT invalidate historical task records that already use that status.

#### Scenario: Read a task with a retired status

- **WHEN** a task record contains a status that is no longer enabled for new transitions
- **THEN** the system continues to expose the historical status
- **AND** does not silently rewrite the task to another status

### Requirement: Distinguish completion and cancellation outcomes

Each configured workflow status MUST map to no outcome, `completed`, or `cancelled`. The task MUST store its current outcome separately from its workflow status.

Entering a status mapped to `completed` or `cancelled` MUST set the corresponding task outcome. Moving to a status with no outcome mapping MUST clear the current outcome. Configuration edits MUST NOT retroactively rewrite existing task outcomes.

#### Scenario: Inspect terminal outcome

- **WHEN** a client inspects a task with a completion or cancellation outcome
- **THEN** the returned task data identifies which outcome occurred

#### Scenario: Reopen a completed or cancelled task

- **WHEN** a task transitions from a status mapped to a terminal outcome to a status with no outcome mapping
- **THEN** the task's current outcome is cleared
- **AND** the previous outcome remains available in immutable operation history
