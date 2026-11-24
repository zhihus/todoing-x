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
- scope: `task` or `project` (default `task`);
- creation timestamp;
- active/removal state.

Task scope enforces one active row per `(task_id, namespace, value)`. Project scope
enforces one active row per `(namespace, value)` across the project store and
conflicts with any other active reference using that pair. The checks must be
transactionally safe under concurrent writers. Todoing does not interpret namespace
or relation. Removal marks a reference inactive rather than erasing its history. Every add/remove operation is
recorded in operation history and reflected in the new task version snapshot.
Adding an already-active identical reference is a no-op. Removing an already
inactive reference is also a no-op; an unknown reference ID returns a stable
not-found error. Reusing an active namespace/value pair with a different relation on the same task
is a conflict.

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
Repeating the same SHA is a no-op. A different SHA may replace the current marker
only after the task has been explicitly reopened; the previous value remains in
immutable versions and history. No clear operation is provided.

The operation honors active task-lock ownership: an unlocked task may be modified without a token; when locked, only its owner may change the marker. A terminal task must be reopened
before the marker can be changed. The final marker is traceability metadata and
does not change task outcome or workflow status.

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
conflicts, missing reviewed versions, and unknown external-reference IDs.

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

## SQLite snapshot verification and restore

Snapshot creation remains the existing SQLite Online Backup API operation.
Add explicit verification and restore operations without introducing backup
scheduling, retention, replication, or incremental snapshots.

- `store.snapshot.verify` opens a separate snapshot and validates project
  identity, the supported schema and migration checksums, SQLite integrity,
  foreign-key integrity, file size, and SHA-256.
- `store.snapshot.restore` restores the complete database through SQLite
  Online Backup API into the existing database connection; it does not replace
  the database file. The source must be a separate, verified snapshot with the
  same project ID and supported schema.
- Restore is rejected while any task lock is active. It acquires an exclusive
  SQLite lock before checking lock state and keeps that lock through restore.
- Before mutation, Todoing creates a verified safety snapshot in
  `.todoing/backups/`. The safety snapshot is retained after success and is
  used as the rollback source if restore or post-copy integrity verification
  fails.
- Restore does not roll back Git or consumer-owned artifacts such as OpenSpec.
  It restores only the Todoing SQLite store.

The operation is intended for restoring a valid project store to a prior
verified snapshot. The current database must be openable by Todoing so the
normal store context can be established; recovery from a database that cannot
be opened at all remains an offline/manual recovery procedure.

## Testing strategy

Write tests before implementation for:

- valid add/list/remove, task/project uniqueness scopes, and external-reference no-op behavior;
- lock owner and non-owner access;
- reference no-op and historical snapshot behavior;
- Review version binding and outcomes;
- exact idempotency and conflicting duplicate review IDs;
- revision-count increments once per Review record;
- final commit resolution, no-op, replacement after reopen, and rejection before reopen;
- transaction rollback at each mutation boundary;
- schema v2 to v3 migration with existing data;
- snapshot/verify/restore round-trip, checksum mismatch, active-lock refusal, and safety-snapshot preservation;
- JSON request/response schemas and stable error codes.

Run the full existing test suite and release build on both Windows and Linux CI.
