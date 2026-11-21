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
