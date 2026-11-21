# Task locking

## Requirements

### Requirement: Exclusive task work lock

The system MUST provide an exclusive lock for a task for the full duration of an agent's work session.

While a task is locked by one owner, other clients MUST be prevented from performing task operations on it, including reads. The lock owner remains authorized to access the task.

#### Scenario: Acquire an available task lock

- **WHEN** a client acquires a lock on an unlocked task
- **THEN** the system records the lock owner
- **AND** grants exclusive access to that owner

#### Scenario: Reject a competing client

- **WHEN** a different client attempts to read or mutate a locked task
- **THEN** the operation is rejected with a predictable lock-conflict result

### Requirement: Identify and renew lock ownership

A lock MUST identify its owner and support heartbeat or lease renewal while work continues.

The lock protocol MUST support explicit release by its owner.

#### Scenario: Renew an active lock

- **WHEN** the lock owner sends a valid heartbeat before lease expiration
- **THEN** the system extends the lock according to the configured or defined lease policy

#### Scenario: Release a lock

- **WHEN** the lock owner releases its lock
- **THEN** the lock becomes available to another client

### Requirement: Recover abandoned locks

The system MUST define a timeout or lease-expiration policy that allows recovery of a lock abandoned after abnormal process termination.

A lock MUST NOT be recovered while its valid owner continues to renew it.

#### Scenario: Recover an expired lock

- **WHEN** a lock lease expires and cannot be renewed by its owner
- **THEN** the system makes the task available for a new lock acquisition
- **AND** records or exposes the recovery result
