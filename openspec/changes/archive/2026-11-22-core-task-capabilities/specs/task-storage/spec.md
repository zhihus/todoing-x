## ADDED Requirements

### Requirement: Atomic task-state transaction

The system MUST commit a task mutation, its new task version, the corresponding operation-history entry, and all state required to expose the committed task consistently as one logical transaction.

#### Scenario: Commit a task mutation

- **WHEN** a valid task mutation succeeds
- **THEN** the task state, new version, operation-history entry, and related state become visible together

#### Scenario: Roll back a failed task mutation

- **WHEN** a failure occurs before the logical transaction commits
- **THEN** none of the mutation's task, version, history, or related state changes become visible
- **AND** the previously committed task state remains intact

### Requirement: Preserve data integrity across concurrent clients

The system MUST prevent concurrent clients from silently overwriting committed task changes or allocating duplicate task identifiers.

#### Scenario: Concurrent task creation

- **WHEN** multiple clients create tasks concurrently in the same project store
- **THEN** every committed task receives a unique stable identifier
- **AND** no committed task is silently overwritten

### Requirement: Safe migration from the current task store

The system MUST provide a validated migration path from the current Markdown/YAML task files and JSON index.

Migration MUST preserve task identifiers, titles, creation timestamps, statuses, labels, bodies, Git commit references, and all other supported metadata, or report data that cannot be migrated before any destructive action.

#### Scenario: Validate migration input

- **WHEN** migration is run against an existing Todoing store
- **THEN** the source records are validated
- **AND** conflicts and unsupported data are reported with enough detail for recovery
- **AND** source data remains available until migration integrity has been verified

#### Scenario: Verify migrated records

- **WHEN** migration reports success
- **THEN** the migrated task count and preserved task data can be checked against the source
- **AND** no source task identifier or supported metadata has been silently lost
