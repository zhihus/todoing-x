# Task statuses

## ADDED Requirements

### Requirement: User-configurable status definitions

Todoing MUST load task workflow status definitions from a user-editable project configuration. Status definitions MUST be user-controlled and MUST NOT be hard-coded to a single built-in workflow.

#### Scenario: Load project status configuration

- **WHEN** Todoing opens a project task store with a valid status configuration
- **THEN** it makes the configured statuses available to task operations
- **AND** preserves their configured ordering

### Requirement: Validate permitted status transitions

Todoing MUST represent and validate permitted transitions between configured statuses.

#### Scenario: Accept a permitted transition

- **WHEN** a task is moved from its current status to a status permitted by the project configuration
- **THEN** the transition is accepted

#### Scenario: Reject a prohibited transition

- **WHEN** a task is moved to a status that is not permitted from its current status
- **THEN** the transition is rejected
- **AND** the task state remains unchanged

### Requirement: Preserve historical statuses when disabled

Disabling a status MUST prevent its use in new transitions without deleting or rewriting historical task records that already use that status.

#### Scenario: Read a task with a disabled status

- **WHEN** a stored task uses a status that has since been disabled
- **THEN** Todoing continues to expose the stored status
- **AND** rejects a new transition into that disabled status

### Requirement: Distinguish completion and cancellation outcomes

Todoing MUST represent completion and cancellation as distinct semantic outcomes. It MUST NOT silently treat a cancelled task as a completed task or conflate the two outcomes.

#### Scenario: Preserve a cancelled outcome

- **WHEN** a task is cancelled
- **THEN** its cancellation outcome remains distinguishable from successful completion in task data and client responses
