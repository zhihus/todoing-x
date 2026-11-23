# Generic task references, review history, and final commit marker

## Why

The current JSON API v1 provides task versions, operation history, dependencies,
and ordinary Git commit references. It does not provide the generic external links,
structured Review history with `revision_count`, or an explicit final commit marker
required by previously approved consumer requirements.

These capabilities must be implemented in Todoing as reusable task-management
mechanisms. Todoing must remain independent of Manifest, OpenSpec, and any agent
harness.

## What Changes

- Add typed, project-local external references to tasks with add/list/remove API
  operations and uniqueness for active `namespace + value` pairs.
- Add append-only Review records linked to immutable task versions.
- Maintain `revision_count` as task metadata, incrementing exactly once for each
  new `changes_requested` Review record.
- Make Review creation idempotent by `review_id`.
- Add a distinct final Git commit marker, separate from ordinary commit references.
- Extend JSON API v1 schemas, operation documentation, error catalogue, and tests.
- Migrate existing SQLite stores non-destructively to the new schema.

## Capabilities

### New capabilities

- `task-external-references`: generic typed links from tasks to external artifacts.
- `task-review-history`: append-only version-linked review records and revision count.
- `task-final-commit-reference`: one explicit final Git commit reference per task.

### Modified capabilities

- `task-json-api`: additive API v1 operations and response fields.
- `task-history-versions`: include new task metadata in immutable snapshots and
  preserve atomicity for Review/reference mutations.
- `task-storage`: schema migration and integrity verification for the new records.

## Out of Scope

- Manifest-specific or OpenSpec-specific fields, namespaces, status names, or policy.
- Actor authentication and authorization beyond existing task-lock ownership.
- Generic lifecycle hooks/events or event delivery.
- Changing task workflow status when a Review record is added.
- Treating any commit reference as proof of task completion.
- Changing existing API v1 request-envelope semantics.

## Compatibility and migration

The extension is additive within JSON API v1. Existing requests and response fields
remain compatible. Existing task records, versions, history, locks, baselines,
dependencies, archive metadata, and commit references must be preserved.

A numbered SQLite migration must add the new tables/columns and indexes without
rewriting or discarding existing task data. Existing stores must migrate
automatically through the established migration path and pass integrity verification.
Snapshot and migration tests must cover the new schema.

## Decisions

- External references are generic: Todoing stores `namespace`, `value`, and
  `relation`, but does not interpret them.
- An active `namespace + value` pair is unique within one project store.
- Review records contain `review_id`, reviewed version, reviewer label, outcome,
  comments, actor attribution, and timestamp.
- Same `review_id` plus identical payload is an idempotent no-op; a different
  payload is rejected.
- `revision_count` is persisted in task state and increases once per newly accepted
  `changes_requested` record, regardless of comment count.
- A Review record does not change workflow status.
- A task has one current final commit SHA. Repeating the same SHA is a no-op;
  replacing it is allowed only after the task is explicitly reopened. Prior values
  remain in immutable versions and history.
- Adding/removing external references, adding Review records, and setting the final
  commit marker require the current task lock and create a version/history event
  atomically when state changes.

## Verification

The implementation must be validated by focused unit/API tests, the complete
`cargo test` suite, release build, and the existing Windows + Unix-like CI matrix.
The OpenSpec change must validate before implementation is considered complete.
