# Transactional task storage

## ADDED Requirements

### Requirement: Atomic task mutation

Todoing MUST commit a task mutation and all state changes required to represent that mutation as one atomic logical operation.

A logical task mutation includes its task data, new task version, operation-history entry, and associated indexes or state needed to expose the committed task consistently.

#### Scenario: Commit a successful mutation

- **WHEN** a task mutation and all associated version, history, and state updates succeed
- **THEN** the complete new logical state becomes visible

#### Scenario: Roll back a failed mutation

- **WHEN** any part of a task mutation or its associated version, history, or state update fails before commit
- **THEN** none of the partial changes become visible as a committed mutation
- **AND** the previously committed task state remains available

### Requirement: Preserve task data integrity independently of Git

Todoing MUST preserve task data integrity without requiring the project to be a Git repository or relying on Git operations for transaction correctness.

#### Scenario: Operate outside Git

- **WHEN** Todoing performs task operations in a project that is not under Git
- **THEN** the same task integrity and atomicity guarantees apply

### Requirement: Prevent silent concurrent overwrites and duplicate identifiers

Todoing MUST coordinate concurrent task creation and mutation so that task identifiers are not allocated more than once and committed changes are not silently overwritten.

#### Scenario: Concurrent task creation

- **WHEN** multiple clients attempt to create tasks concurrently
- **THEN** each successful task receives a distinct stable identifier
