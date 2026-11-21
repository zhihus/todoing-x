## ADDED Requirements

### Requirement: Exclusive task work lock

The system MUST support an exclusive lock for a task for the duration of a client's work session. While the lock is active, clients other than the lock owner MUST NOT read or mutate the locked task.

#### Scenario: Exclude another client

- **WHEN** client A holds the active lock for a task and client B attempts to read or mutate that task
- **THEN** the operation by client B is rejected with a machine-identifiable lock conflict
- **AND** client A retains access while its lock remains valid

### Requirement: Maintain and release a task lock

The lock protocol MUST support acquisition, owner identity, heartbeat or lease renewal, and explicit release.

#### Scenario: Renew an active lock

- **WHEN** the lock owner renews a valid lock
- **THEN** the lock remains owned by that client for the renewed lease

#### Scenario: Release an active lock

- **WHEN** the lock owner explicitly releases its lock
- **THEN** the lock becomes available for another client

### Requirement: Recover abandoned task locks

The system MUST provide a defined recovery mechanism for a lock whose owner terminates abnormally and can no longer renew it.

#### Scenario: Recover an expired lock

- **WHEN** a lock is determined to be abandoned under the configured recovery policy
- **THEN** the system permits recovery without silently transferring ownership while the original lease is valid
- **AND** subsequent operations observe one consistent lock owner

#### Scenario: Hide locked tasks from list results

- **WHEN** client A holds an active task lock and client B lists or searches tasks
- **THEN** the locked task is omitted entirely from client B's results
- **AND** its ID and task fields are not disclosed

#### Scenario: Protect task history and versions

- **WHEN** client B requests history, versions, labels, commit references, or dependencies for a task locked by client A
- **THEN** the request is rejected or returns only an opaque locked dependency reference
- **AND** no protected task content is disclosed

#### Scenario: Reject a stale owner after recovery

- **WHEN** client B recovers an expired lock after the configured grace period
- **THEN** the fencing generation increases
- **AND** subsequent reads and mutations from client A using its old token/generation are rejected
