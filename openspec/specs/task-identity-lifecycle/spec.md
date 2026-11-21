# Task identity and lifecycle

## Purpose

Describe stable task identity and terminal task outcomes in the SQLite task model.

## Requirements

### Requirement: Stable task identifiers

The system MUST assign each task a stable identifier that is unique within its project store. An identifier MUST NOT be reused after completion, cancellation, or removal from active work.

#### Scenario: Create a task after another task becomes terminal

- **WHEN** a new task is created after another task has completed or been cancelled
- **THEN** the new task receives a different identifier
- **AND** references to the earlier task remain unambiguous

### Requirement: Prohibit physical task deletion

The supported Todoing interfaces MUST NOT physically delete task records. Completion and cancellation MUST NOT physically delete task data, relationships, versions, or history.

#### Scenario: Request physical deletion

- **WHEN** a client requests deletion of a task
- **THEN** Todoing rejects the request
- **AND** the task record and related data remain intact

### Requirement: Represent terminal outcomes explicitly

The SQLite task model MUST represent `completed` and `cancelled` as distinct nullable outcomes independent of workflow status. A task with a terminal outcome MUST be reopened before ordinary status changes.

#### Scenario: Complete a task

- **WHEN** an active task is completed
- **THEN** its outcome becomes `completed`
- **AND** its workflow status is retained

#### Scenario: Cancel a task

- **WHEN** an active task is cancelled
- **THEN** its outcome becomes `cancelled`
- **AND** its workflow status is retained
- **AND** an optional cancellation reason is recorded in history

#### Scenario: Reopen a task

- **WHEN** a completed or cancelled task is explicitly reopened
- **THEN** its outcome is cleared
- **AND** any requested workflow status transition is validated against project configuration
