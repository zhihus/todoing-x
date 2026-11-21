# Core task capabilities

## Why

The current Rust CLI is a compatibility prototype. Its Markdown files and JSON index do not provide the transactional integrity, configurable workflow, dependency model, immutable history, versioning, or exclusive work locks required by the accepted product baseline.

This change defines the target behavior for the core MVP without selecting a storage engine, schema, lock algorithm, or JSON envelope prematurely.

## What Changes

- Define user-configurable task statuses and validated transitions without embedding Manifest's workflow in Todoing.
- Require stable task identifiers and prohibit physical task deletion and identifier reuse.
- Add task dependencies with cycle detection and queryable dependency state.
- Add immutable operation history and task versions, including an identifiable user-approved baseline.
- Require exclusive task locks for the duration of an agent work session, including read exclusion for other clients.
- Define transactional persistence across task state, versions, history, and related indexes.
- Define a stable, versioned JSON contract and predictable machine-readable errors.
- Require a safe migration path from the current Markdown/YAML files and JSON index.

## Capabilities

### New capabilities

- `task-status-configuration`: user-controlled statuses and permitted transitions.
- `task-dependencies`: task relationships, cycle prevention, and dependency state.
- `task-history-versions`: append-only operation history, task versions, approval baseline, and comparison.
- `task-locking`: exclusive work-session locks, heartbeat, release, and recovery.
- `task-identity-lifecycle`: stable identifiers and prohibition of physical deletion.
- `task-json-api`: stable, versioned machine interface and predictable errors.

### Modified capabilities

- `task-storage`: add logical transaction guarantees and safe migration requirements beyond the current per-file atomic replacement behavior.

## Out of Scope

- Selecting SQLite or another storage engine.
- Defining physical database schema, serialization layout, lock lease duration, or heartbeat interval.
- Defining the exact JSON envelope, error-code catalogue, or CLI command syntax.
- Manifest-specific status names, OpenSpec rules, Git workflow, or harness behavior.
- Archive/restore implementation, generic lifecycle-event delivery, full import/export, backup automation, and UI; these remain post-MVP unless separately reprioritized.

## Compatibility and migration

The existing Rust CLI, Markdown/YAML task records, JSON index, and Git commit references must be inventoried and preserved through an explicit migration design. No implementation in this change may silently discard user data or perform physical task deletion.

## Decisions still required

The technical design must resolve the open questions recorded in `docs/architecture/decisions.md`, including status/outcome representation, actor identity, API versioning details, storage engine, transaction boundaries, lock recovery semantics, and migration strategy.
