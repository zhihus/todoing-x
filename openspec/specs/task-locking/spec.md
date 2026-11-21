# Task locking

## Purpose

Describe exclusive task work sessions, leases, read/write exclusion, and abandoned-lock recovery in the SQLite backend.

## Requirements

### Requirement: Exclusive task work lock

The system MUST support an exclusive lock for a task for the duration of a client's work session. While the lock is active, clients other than the owner MUST NOT read or mutate the locked task.

#### Scenario: Exclude another client

- **WHEN** client A holds an active lock and client B attempts to read or mutate the task
- **THEN** client B's operation is rejected with a machine-identifiable lock conflict
- **AND** client A retains access while its lock remains valid

### Requirement: Hide locked tasks and protect related data

A task locked by another client MUST be omitted from list/search results. Task-specific history, versions, baselines, labels, commit references, and dependencies MUST be protected. A locked dependency MAY be represented only as an opaque locked reference.

#### Scenario: List while a task is locked

- **WHEN** client A holds an active lock and client B lists or searches tasks
- **THEN** the locked task and its ID are omitted from the results

#### Scenario: Read related task data while locked

- **WHEN** client B requests protected data for a task locked by client A
- **THEN** the request is rejected or returns only an opaque locked dependency reference
- **AND** protected task content is not disclosed

### Requirement: Maintain and release a task lock

The lock protocol MUST support acquisition, owner identity, lease expiry, heartbeat renewal, explicit release, and a fencing generation. The bearer token MUST be generated securely; only its SHA-256 hash is persisted.

#### Scenario: Acquire a task lock

- **WHEN** a client acquires an unlocked task
- **THEN** the system returns an owner ID, one-time bearer token, fencing generation, and lease timestamps
- **AND** stores only the token hash

#### Scenario: Renew an active lock

- **WHEN** the owner renews a valid lock with matching token and fencing generation
- **THEN** the lease is extended

#### Scenario: Release an active lock

- **WHEN** the owner explicitly releases its current lock
- **THEN** the lock becomes available for a subsequent acquisition
- **AND** its fencing generation is retained

### Requirement: Recover abandoned task locks

The system MUST provide explicit recovery only after the lease expires and the configured recovery grace period elapses. Recovery MUST NOT silently transfer ownership while the original lease is valid. A successful recovery MUST increment the fencing generation.

#### Scenario: Recover an expired lock

- **WHEN** a client recovers a lock after expiry plus recovery grace
- **THEN** a new owner and token are issued
- **AND** the fencing generation increases

#### Scenario: Reject a stale owner

- **WHEN** the previous owner uses an old token or fencing generation after recovery
- **THEN** its subsequent reads and mutations are rejected

### Requirement: Preserve lock operation history

Acquisition, heartbeat, release, recovery, and rejected lock operations MUST append history events without creating a task version unless task state also changes.

#### Scenario: Record a lock lifecycle event

- **WHEN** a lock lifecycle operation is accepted or rejected
- **THEN** the event is appended to operation history
