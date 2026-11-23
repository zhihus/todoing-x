# Design: generic task references, Review history, and final commit marker

## Design constraints

- Todoing remains an independent, local-first Rust CLI.
- No Manifest, OpenSpec, or harness-specific concepts may enter the core.
- Existing API v1 clients remain compatible; new operations are additive.
- Existing task data and immutable history must remain intact.
- SQLite schema changes use numbered, checksummed migrations.
- Windows and Unix-like behavior must be covered by CI.
- Task-specific reads and writes obey existing lock read-exclusion rules.

## External references

Add a task-scoped external-reference model with:

- stable record ID;
- task ID;
- namespace (consumer-defined, non-empty bounded string);
- value (non-empty bounded string);
- relation (consumer-defined, non-empty bounded string);
- creation timestamp;
- active/removal state.

A partial unique index enforces one active row per `namespace + value` in a
project store. Todoing does not interpret namespace or relation. Removal marks a
reference inactive rather than erasing its history. Every add/remove operation is
recorded in operation history and reflected in the new task version snapshot.
Adding an already-active identical reference is a no-op.

The API returns active references by default. Historical task versions retain the
reference set that existed at that version.

## Review history

Add an append-only Review table. Each record contains:

- monotonically increasing record ID;
- task ID;
- stable caller-provided review ID;
- reviewed task version number;
- reviewer display/role label;
- outcome: `approved` or `changes_requested`;
- ordered comments array stored as validated JSON;
- actor ID from the request envelope;
- timestamp.

A unique constraint on `(task_id, review_id)` supports idempotency. On a repeated
ID, compare the complete canonical payload, including actor attribution. An exact
match returns the original record with `no_op=true`; a mismatch returns a stable
conflict error without mutation.

The reviewed version must exist and belong to the task. A new record increments
the task version, appends operation history, and updates `revision_count` in the
same SQLite transaction. Only `changes_requested` increments the counter, exactly
once per record. Multiple comments in one record count once. A Review operation
does not alter workflow status.

The task response exposes `revision_count`; `task.review.list` exposes the
append-only records with bounded pagination. Task version snapshots include the
counter and review history visible at that version.

## Final commit marker

Add nullable `final_commit_sha` task metadata. `task.commit.finalize` resolves a
Git revision using the existing repository resolution rules and stores the full
SHA. The first successful assignment creates a task version and history event.
Repeating the same SHA is a no-op. A different SHA when one is already set is
rejected; no implicit replacement or clear operation is provided.

The operation requires the current task lock. The final marker is traceability
metadata and does not change task outcome or workflow status.

## API and errors

Add operations:

- `task.external-ref.add`
- `task.external-ref.list`
- `task.external-ref.remove`
- `task.review.add`
- `task.review.list`
- `task.commit.finalize`

Request/response schemas and operation documentation must define exact required
fields, bounds, pagination, no-op responses, and stable error codes. Error cases
must distinguish duplicate external-reference conflicts, review-ID payload
conflicts, missing reviewed versions, and conflicting final commit assignment.

All task-specific operations must enforce existing lock ownership. A non-owner
must not learn protected task/reference/review data.

## Storage migration

Add a new numbered migration after the current schema version. It must:

- create external-reference and Review tables with foreign keys;
- add `revision_count` and `final_commit_sha` task columns with safe defaults;
- add indexes and uniqueness constraints;
- preserve all existing rows and snapshots;
- record the migration checksum and update schema metadata atomically.

Store verification must require the new schema objects and verify migration
checksums. Snapshot and restore must preserve all new data.

## Failure and recovery

Each mutation uses one immediate SQLite transaction for task state, version,
operation history, and the associated record. A failure rolls back all changes.
A duplicate idempotent request must not create a second version or counter increment.
Integrity verification must detect missing schema objects, invalid foreign keys,
and migration checksum mismatch.

## Testing strategy

Write tests before implementation for:

- valid add/list/remove and external-reference uniqueness;
- lock owner and non-owner access;
- reference no-op and historical snapshot behavior;
- Review version binding and outcomes;
- exact idempotency and conflicting duplicate review IDs;
- revision-count increments once per Review record;
- final commit resolution, no-op, and conflicting replacement;
- transaction rollback at each mutation boundary;
- schema v2 to v3 migration with existing data;
- snapshot/verify round-trip;
- JSON request/response schemas and stable error codes.

Run the full existing test suite and release build on both Windows and Linux CI.
