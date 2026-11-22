# Task dependencies

## Purpose

Describe the dependency relationships supported by the SQLite task repository.

## Requirements

### Requirement: Manage task dependencies

The system MUST allow a task to declare dependencies on other tasks in the same project store. Clients MUST be able to inspect dependency relationships and dependency state.

#### Scenario: Add a dependency

- **WHEN** a client adds an existing task as a dependency of another task
- **THEN** the relationship is persisted
- **AND** it appears in the task's dependency list and version snapshot

#### Scenario: Remove a dependency

- **WHEN** a client removes an existing dependency
- **THEN** the relationship is removed
- **AND** the task receives a new version and successful history event

#### Scenario: Inspect dependency state

- **WHEN** a client lists a task's dependencies
- **THEN** the response identifies each dependency and its current state
- **AND** does not silently omit unresolved dependencies

### Requirement: Preserve archived dependencies

Archiving a task MUST NOT remove dependency relationships. Dependency inspection MUST continue to return the referenced task and identify whether it is archived, without exposing protected fields of a locked dependency.

#### Scenario: Inspect an archived dependency

- **WHEN** a task depends on an archived task that is not locked
- **THEN** the dependency remains in the relationship list
- **AND** its state reports `archived=true`

### Requirement: Prevent dependency cycles

The system MUST reject a dependency change that would create a direct or indirect cycle in the task dependency graph. The cycle check and relationship mutation MUST occur in one transaction.

#### Scenario: Reject a direct cycle

- **WHEN** a dependency would make task A depend on task B while task B already depends on task A
- **THEN** the operation is rejected
- **AND** committed relationships remain unchanged

#### Scenario: Reject an indirect cycle

- **WHEN** a dependency change would create a cycle involving three or more tasks
- **THEN** the operation is rejected
- **AND** no partial relationship changes are committed

### Requirement: Protect locked dependency references

A dependency on a task locked by another client MUST be represented without disclosing the locked task's protected content.

#### Scenario: List a locked dependency

- **WHEN** a client lists dependencies and one dependency is locked by another client
- **THEN** the response exposes only an opaque locked reference
- **AND** does not disclose the locked task's title, body, labels, or other protected fields
