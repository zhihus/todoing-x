# Design: core task capabilities

## Design constraints

- Todoing remains a standalone, local-first Rust application.
- The current Markdown/YAML plus JSON-index implementation is the compatibility source to migrate from, not a mandatory target storage design.
- Storage, API, and locking decisions must be implementation-neutral in this specification.
- The design must work on supported Windows and Unix-like platforms.
- Manifest is an external consumer. No Manifest-specific status, OpenSpec, Git, or harness policy may enter Todoing's core.

## Task identity and lifecycle

Task identifiers are stable for the lifetime of the project store and are never reused. A task cannot be physically deleted through the supported product interface. Archival is a separate lifecycle operation and is not part of this MVP change.

## Status model

A project defines its own statuses and permitted transitions. Todoing validates status definitions and requested transitions against that configuration. Historical records remain readable if a status is later disabled; disabling a status prevents new transitions into it.

Completion and cancellation are distinct outcomes. The exact relationship between outcomes and configurable statuses is intentionally unresolved here and must be decided before implementation. The model must not assume that a status literally named `done` or `cancelled` is universal.

## Dependencies

Dependencies are directed task relationships. The system rejects any operation that would introduce a dependency cycle and exposes each task's dependency relationships and current dependency state. This change does not define whether a particular dependency state blocks a status transition; that policy requires an explicit product decision.

## History and versions

Every successful task save creates a new immutable version. A separate append-only operation history records the actor, timestamp, operation, previous value, new value, and outcome. A user-approved version is identifiable as the task's baseline. Clients can inspect versions and compare any two versions.

The representation of actor identity, version snapshots, diff semantics, and whether rejected attempts are recorded as history events must be resolved in design. A rejected operation must never create a successful task version or partially mutate task state.

## Locking

A task lock is exclusive for the full work session. A client that does not own the lock cannot read or mutate that task while the lock is active. The owner can continue working and renew the lock. The protocol must support explicit release and recovery after abnormal process termination.

The lease/timeout algorithm, owner identity format, heartbeat cadence, and recovery race handling require technical design. The implementation must not rely on a process-local mutex as the only concurrency control.

## Transactional storage

A task mutation, its new version, the corresponding history entry, and all state required to expose the committed task consistently form one logical transaction. A failure before commit leaves the prior committed state intact; a successful commit makes all related records visible together.

The storage engine is not selected. The evaluation must cover transaction semantics, multi-process concurrency, filesystem behavior, backup/restore, portability, performance, and operational complexity.

## JSON contract

The machine interface is versioned, documented, deterministic, and backward-compatible within a declared API version. Machine-readable errors use stable codes and predictable process exit codes. Diagnostics must not contaminate JSON output on stdout.

The exact envelope, version negotiation, error catalogue, and command-to-API mapping are open design items.

## Migration

Migration from Markdown/YAML task files and the JSON index must preserve task identifiers, titles, timestamps, statuses, labels, bodies, Git references, and any other supported metadata. The process must validate source data, report conflicts and invalid records, and provide a recovery path. Source data must not be destructively removed as part of migration.

## Platform and verification

The design must account for Windows and Unix-like filesystem and process semantics. Tests must cover transaction rollback, crash/failure boundaries, concurrent clients, lock expiry/recovery, dependency cycles, migration integrity, and API compatibility. Cross-platform CI must validate supported behavior.
