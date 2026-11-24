## MODIFIED Requirements

### Requirement: Create immutable task versions

Every successful task-state save MUST create a new immutable task version in the
same transaction as the task mutation and successful history event. External
references, Review history, revision_count, and the final commit marker MUST be
included in version snapshots. Failed or rejected saves MUST NOT create successful
versions.

#### Scenario: Save a task

- **WHEN** a task mutation is successfully committed
- **THEN** a new immutable version is created
- **AND** all previous versions remain unchanged

#### Scenario: Snapshot an external-reference mutation

- **WHEN** a lock owner adds or removes an external reference
- **THEN** the resulting task version records the reference state at that point
- **AND** the task mutation, version, and history event commit atomically

#### Scenario: Snapshot a Review mutation

- **WHEN** a lock owner adds a new Review record
- **THEN** the resulting task version includes the Review record and revision_count
- **AND** the Review record, task version, and history event commit atomically

#### Scenario: Snapshot the final commit marker

- **WHEN** a lock owner sets the final commit marker
- **THEN** the resulting task version includes the resolved final SHA
- **AND** the task mutation, version, and history event commit atomically
