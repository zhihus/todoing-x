# Transactional storage

## Requirements

### Requirement: Atomic task mutation

A successful task mutation MUST be committed as one logical transaction that includes the task state, its new version, the corresponding history entry, and all associated indexes or derived state.

The transaction MUST either commit all of these changes or leave the previously committed state intact.

#### Scenario: Commit a task mutation

- **WHEN** a task mutation completes successfully
- **THEN** the task state, version, history entry, and associated state are all committed

#### Scenario: Fail during a task mutation

- **WHEN** an error occurs before the logical transaction commits
- **THEN** none of the transaction's changes become visible as committed state
- **AND** the previously committed task state, version, and history remain intact

### Requirement: Recover from interrupted operations

The system MUST preserve transaction integrity after abnormal process termination or system interruption.

After recovery, clients MUST observe either the complete pre-operation state or the complete committed post-operation state, never a partially applied logical mutation.

#### Scenario: Recover after interruption

- **WHEN** the process or system stops during a task mutation
- **THEN** the next valid store access performs or uses recovery as required
- **AND** exposes only a transactionally consistent state

### Requirement: Protect concurrent mutations

The system MUST prevent concurrent clients from silently overwriting task changes or allocating duplicate task identifiers.

Conflicting mutations MUST be serialized or rejected with a predictable conflict result.

#### Scenario: Concurrent task creation

- **WHEN** multiple clients create tasks concurrently
- **THEN** each successfully created task receives a unique identifier

#### Scenario: Concurrent updates to one task

- **WHEN** two clients attempt conflicting mutations of the same task
- **THEN** the system prevents a silent lost update
- **AND** reports a deterministic success or conflict result
