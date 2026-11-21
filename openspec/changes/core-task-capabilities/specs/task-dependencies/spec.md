# Task dependencies

## ADDED Requirements

### Requirement: Associate dependencies with tasks

Todoing MUST allow a task to depend on other tasks and MUST persist those relationships.

#### Scenario: Add a dependency

- **WHEN** a client adds an existing task as a prerequisite of another task
- **THEN** Todoing stores the dependency relationship
- **AND** exposes the relationship to clients

### Requirement: Prevent dependency cycles

Todoing MUST detect and reject dependency changes that would create a cycle.

#### Scenario: Reject a direct or indirect cycle

- **WHEN** adding or changing a dependency would make a task depend on itself directly or through another task
- **THEN** the operation is rejected
- **AND** the existing dependency graph remains unchanged

### Requirement: Expose dependency state

Todoing MUST expose each task's dependency relationships and the current state of its prerequisites through its public client interface.

#### Scenario: Inspect a task with prerequisites

- **WHEN** a client inspects a task that has dependencies
- **THEN** the response identifies its prerequisites
- **AND** provides their current task states

The aggregate rule for whether a dependent task is considered blocked, including whether cancellation satisfies a prerequisite, remains an explicit design decision.
