# Task dependencies

## Requirements

### Requirement: Store task dependencies

The system MUST allow a task to declare dependencies on other tasks in the same project task store.

Dependency relationships MUST use stable task identifiers and MUST be exposed through the public task interface.

#### Scenario: Add a dependency

- **WHEN** a client adds an existing task as a dependency of another task
- **THEN** the relationship is persisted
- **AND** the relationship is visible when either relevant task is inspected

#### Scenario: Reject an unknown dependency

- **WHEN** a client attempts to add a dependency on a task identifier that does not exist
- **THEN** the operation is rejected
- **AND** no dependency relationship is created

### Requirement: Prevent dependency cycles

The system MUST detect and reject dependency changes that would create a cycle.

#### Scenario: Reject a direct self-dependency

- **WHEN** a client attempts to make a task depend on itself
- **THEN** the operation is rejected

#### Scenario: Reject an indirect cycle

- **WHEN** a proposed dependency would create a path that leads back to the task being changed
- **THEN** the operation is rejected
- **AND** the existing dependency graph remains unchanged

### Requirement: Expose dependency state

The system MUST expose a task's dependencies and the state of those dependencies through the public interface.

Dependency state MUST be exposed as open, completed, cancelled, or unavailable when the dependency is locked by another owner. Dependency state is informational and MUST NOT automatically block task operations.

#### Scenario: Inspect dependencies

- **WHEN** a client requests task details
- **THEN** the response includes the task's dependency relationships
- **AND** provides enough information to determine the current state of each dependency

#### Scenario: Inspect a dependency on a locked task

- **WHEN** a client inspects a task whose dependency is locked by another owner
- **THEN** the dependency is reported as unavailable
- **AND** the locked task's content and state are not exposed
