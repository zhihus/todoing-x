# Task external references

## ADDED Requirements

### Requirement: Store generic external references

The system MUST allow a task to store zero or more typed external references. Each
reference MUST contain a non-empty namespace, value, and relation, plus a scope
(task or project; default task). Todoing MUST store these values without interpreting
consumer-specific meaning.

#### Scenario: Add an external reference

- **WHEN** a client adds a valid external reference to a task while holding its active lock
- **THEN** the reference is returned by the task reference list
- **AND** the task version and operation history record the change atomically

### Requirement: Enforce active reference uniqueness

With task scope, an active `namespace + value` pair MUST be unique within one task.
With project scope, the pair MUST be unique across the project store and conflict
with any other active reference using that pair.

#### Scenario: Enforce task-scope uniqueness

- **WHEN** a client adds the same namespace/value pair to the same task twice
- **THEN** the second identical add is a no-op
- **AND** no new task version is created

#### Scenario: Enforce project-scope uniqueness

- **WHEN** a client adds a project-scoped pair already active on another task
- **THEN** the operation is rejected with external_ref_conflict
- **AND** neither task is changed

#### Scenario: Treat an identical add as a no-op

- **WHEN** a lock owner repeats an identical active reference on the same task
- **THEN** the existing reference is returned with no_op=true
- **AND** no new task version is created

#### Scenario: Reject a duplicate active reference

- **WHEN** a client adds an active namespace/value pair already attached to another task
- **THEN** the operation is rejected with external_ref_conflict
- **AND** neither task is changed

### Requirement: Remove references without erasing history

Removing a reference MUST mark it inactive and append an operation-history event.
Historical task versions MUST preserve the reference state that existed when they
were created.

#### Scenario: Remove an active reference

- **WHEN** the lock owner removes an active reference
- **THEN** it is no longer returned by the default active-reference list
- **AND** the task version and operation history record the removal

#### Scenario: Repeat removal or use an unknown reference ID

- **WHEN** a client repeats removal of an inactive reference
- **THEN** the operation returns no_op=true without a new task version
- **WHEN** a client supplies an unknown reference ID
- **THEN** the operation is rejected with external_ref_not_found

### Requirement: Enforce lock read exclusion

External-reference reads and writes MUST obey task lock ownership. A non-owner MUST
NOT receive reference contents for a locked task.

#### Scenario: Read references for a foreign-locked task

- **WHEN** a client without the valid lock token requests a locked task's references
- **THEN** the request is rejected without disclosing reference data
