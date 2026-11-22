# Design: Task archive and restore

## 1. Product semantics

Archive state is orthogonal to workflow status and terminal outcome:

- A task may be active or archived regardless of whether its outcome is null, completed, or cancelled.
- Archive does not imply completion or cancellation.
- Completion/cancellation does not imply archive.
- Archiving does not remove task identity, history, versions, dependencies, labels, or commit references.
- Restore clears archive state and preserves the task's workflow status and outcome.

No automatic archival is introduced.

## 2. Proposed persistence model

Add nullable `archived_at` and `archived_by` fields to the current task record. Both are null for active tasks and populated atomically when archived. Restore clears both.

The fields are part of user-visible task state and MUST be included in immutable version snapshots. Archive and restore therefore create a new task version and a corresponding history event in the same transaction.

A schema migration adds the columns without rewriting task content. Existing tasks default to active.

## 3. Visibility and mutation rules

- Default task list/search returns active tasks only.
- An explicit archive filter returns archived tasks; an `all` filter may return both.
- Direct task lookup by ID may read an archived task if the caller is not excluded by a task lock.
- Dependency inspection retains archived dependencies and reports their archive state.
- Archiving does not cascade to dependent or prerequisite tasks.
- Archived tasks are read-only until restored. Ordinary task mutations, dependency changes, and new lock acquisition are rejected with a stable `task_archived` error. Restore is the dedicated mutation that reactivates the task.
- Existing lock enforcement remains authoritative. A task held by another client's lock cannot be archived or restored. If the lock owner archives the task, the lock remains until explicitly released or recovered; archive does not silently end a work session.

## 4. Interface

### CLI

- `todoing archive <task-id>`
- `todoing restore <task-id>`
- `todoing ls --archived` lists archived tasks; default listing remains active-only.

These commands require the SQLite backend. On an unmigrated project they fail with the existing migration-required guidance.

### JSON API v1

- `task.archive`: required `task_id`.
- `task.restore`: required `task_id`.
- `task.list`: optional `archive_state` with `active` (default), `archived`, or `all`.
- `task.get`: includes archive metadata.

Archive and restore are idempotent: repeating the operation when the task is already in the requested archive state returns `no_op=true`, appends a successful no-op history event, and does not create a new task version.

## 5. Transaction and history behavior

Archive/restore mutation, archive metadata, immutable version, and history event MUST commit atomically. Rejected attempts MUST leave committed state and version history unchanged and follow existing rejected-operation history behavior.

History operation names are `task.archive` and `task.restore`. Events record actor and timestamp. The archived metadata is included in the version snapshot.

## 6. Compatibility and migration

- Existing migrated databases are upgraded through a numbered schema migration.
- Existing tasks are active after upgrade.
- Legacy Markdown mode is unchanged; archive/restore commands are SQLite-only.
- Database snapshots naturally include archive state.
- Legacy Markdown files retained after migration remain unchanged and are not updated with archive metadata.

## 7. Failure and concurrency cases

Tests must cover transaction rollback, duplicate archive/restore requests, concurrent archive/restore attempts, task lock conflicts, active-list filtering, archive-view filtering, dependency retention, and version/history consistency.

## 8. Open implementation detail

The exact SQL index strategy for active versus archived task listing is an implementation detail. It must be justified by query plans and must not change observable ordering or filtering semantics.
