# Task locking

## ADDED Requirements

### Requirement: Exclusive task lock

Todoing MUST support an exclusive lock on a task for the full duration of a client's work session. While a task is locked, other clients MUST NOT perform task operations on it, including read operations.

The lock owner MUST be able to continue its authorized work on the locked task.

#### Scenario: Reject access by another client

- **WHEN** one client holds a task lock and another client attempts to read or mutate that task
- **THEN** Todoing rejects the second client's operation without changing task data

### Requirement: Track lock ownership and lease activity

A task lock MUST identify its owner and support heartbeat or lease renewal, explicit release, and a defined timeout.

#### Scenario: Renew an active lock

- **WHEN** the lock owner renews its lock within the permitted lease period
- **THEN** Todoing keeps the lock active for the renewed period

#### Scenario: Release a lock

- **WHEN** the lock owner explicitly releases its lock
- **THEN** Todoing makes the task available for a subsequent client to acquire

### Requirement: Recover locks after abnormal termination

Todoing MUST provide a recovery mechanism for locks abandoned by an abnormally terminated client. An abandoned lock MUST NOT block task access indefinitely.

#### Scenario: Recover an expired lock

- **WHEN** a lock expires and its owner can no longer renew it
- **THEN** Todoing makes the task eligible for lock recovery according to the defined recovery protocol

The owner identity, lease duration, heartbeat cadence, and recovery protocol remain architecture decisions.
