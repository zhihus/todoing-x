## ADDED Requirements

### Requirement: Manage task dependencies

The system MUST allow a task to declare dependencies on other tasks in the same project store. Clients MUST be able to inspect the dependency relationships for a task.

#### Scenario: Add a dependency

- **WHEN** a client adds an existing task as a dependency of another task
- **THEN** the relationship is persisted
- **AND** it is available through the supported task interface

### Requirement: Prevent dependency cycles

The system MUST reject a dependency change that would create a direct or indirect cycle in the task dependency graph.

#### Scenario: Reject a direct cycle

- **WHEN** task A is made dependent on task B and task B already depends on task A
- **THEN** the operation is rejected
- **AND** the previously committed relationships remain unchanged

#### Scenario: Reject an indirect cycle

- **WHEN** a dependency change would create a cycle involving three or more tasks
- **THEN** the operation is rejected
- **AND** no partial relationship changes are committed

### Requirement: Expose dependency state

The system MUST expose dependency state to clients, including whether a task has dependencies and the state of those dependencies.

#### Scenario: Inspect dependency state

- **WHEN** a client inspects a task with dependencies
- **THEN** the response identifies its dependencies and their current states
- **AND** does not silently omit unresolved dependencies
