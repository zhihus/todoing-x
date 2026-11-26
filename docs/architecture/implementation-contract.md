# Implementation Contract: Todoing Core MVP

**Status:** Selected technical baseline for implementation.
**Date:** 2026-11-21
**Basis:** Accepted product decisions, candidate evaluation, and cross-platform prototype evidence.
**Scope:** Core MVP. This document resolves the implementation choices left open in target-architecture.md.

## 1. Selected technology

| Area | Selected choice |
|---|---|
| Storage engine | SQLite, one database per project. |
| Rust binding | rusqlite with bundled SQLite. |
| Journal mode | Rollback journal, DELETE. WAL is not used. |
| Durability | synchronous=FULL. |
| Integrity | Foreign keys enabled and verified on every connection. |
| Busy handling | 10-second busy timeout; report a stable storage-busy error if the transaction still cannot proceed. |
| Configuration | TOML, parsed with Serde. |
| Machine API | Versioned JSON request/response through the CLI. |
| Execution model | Synchronous modular monolith; no daemon, server, or async runtime. |
| Platforms | Windows and Linux are required CI targets; Unix-like support remains a design constraint. |

### Rationale

SQLite is selected over redb because the task model is relational: tasks, dependencies, versions, operation history, baselines, and locks require referential integrity and atomic multi-record changes. SQLite provides these directly through transactions, foreign keys, indexes, and recursive queries. redb would require Todoing to implement and maintain more of that machinery itself.

The SQLite process-level proof of concept passed on Windows and Linux. It demonstrated the EXCLUSIVE acquisition barrier in rollback-journal mode, non-owner read exclusion, concurrent ID allocation, rollback after abrupt process termination, and stale-owner fencing.

The production implementation must retain these tests and add repository-level, failure-injection, lease, migration, and backup tests. The proof of concept is evidence for the selected baseline, not a substitute for production verification.

## 2. Project store

Each project has one store under its project root:

~~~text
.todoing/
  todoing.db
  config.toml
  migration/
    reports/
~~~

The database is the only writable source of task state after migration. Legacy Markdown task files and index.json are retained unchanged as read-only recovery material.

The database metadata contains:
- schema version;
- stable project ID;
- store creation timestamp;
- format/API compatibility metadata where required.

todoing init creates a project ID if none is supplied. Manifest-facing JSON API requests MUST include the expected project ID; Todoing compares it with the store metadata and rejects a mismatch. Human CLI use may resolve the store from the current project root without supplying an ID.

Project ID is a namespace guard, not an authentication credential.

## 3. TOML status configuration

The authoritative project workflow configuration is .todoing/config.toml.

### 3.1 Format version 1

~~~toml
format_version = 1

[locking]
lease_seconds = 300
heartbeat_seconds = 60
recovery_grace_seconds = 60

[[statuses]]
id = "todo"
label = "To Do"
enabled = true

[[statuses]]
id = "in_progress"
label = "In Progress"
enabled = true

[[statuses]]
id = "done"
label = "Done"
enabled = true
outcome = "completed"

[[transitions]]
from = "todo"
to = "in_progress"

[[transitions]]
from = "in_progress"
to = "done"
~~~

This is an example configuration, not a mandatory Todoing workflow. A project may replace status IDs, labels, order, and permitted transitions. The order of statuses entries is the presentation order.

### 3.2 Validation rules

- format_version MUST be supported.
- Status IDs MUST be unique, stable lowercase ASCII slugs matching [a-z0-9][a-z0-9_-]* and no longer than 64 characters.
- Labels MUST contain a non-whitespace character.
- Status IDs MUST NOT be silently renamed or reused for a different meaning.
- enabled defaults to true.
- outcome is omitted for ordinary workflow states; supported values are completed and cancelled.
- A status with an outcome sets that outcome when a task enters the status. This is useful for completion statuses such as Done; a cancelled outcome can also be set by the explicit cancellation operation without changing the workflow status.
- A task with a non-null outcome cannot undergo a normal status transition until explicitly reopened.
- Transition endpoints MUST refer to configured statuses.
- A disabled status cannot be a transition target. A task already in a disabled status remains readable and may transition out if that transition is explicitly configured.
- The loader validates the complete file once per CLI/API invocation. All operations in that invocation use the same immutable configuration snapshot.
- The configuration content hash is recorded in status-change history events.
- Configuration edits do not retroactively rewrite task outcomes or historical records.

A project may remove a status from the active workflow only after existing tasks have been moved or the status is retained with enabled = false. Unknown historical status IDs remain visible as raw IDs and require configuration repair before another transition.

### 3.3 Default and migration configuration

For a new project, todoing init writes a generic example configuration with todo, in_progress, and done. It is not hard-coded into the domain model and is not a Manifest-specific workflow.

For legacy migration, the generated compatibility configuration includes todo, in_progress, done, and cancelled; done maps to completed, and the legacy cancelled status maps to cancelled. To preserve the previous CLI's unrestricted status changes, the generated transition set allows every transition between those four statuses. The user can tighten the workflow after migration.

## 4. Domain model

### Task

- task_id: signed 64-bit integer, stable within a project store.
- title: required non-whitespace string.
- status_id: configured status identifier.
- outcome: nullable enum: completed or cancelled.
- body: description text, empty by default.
- created_at, updated_at: UTC RFC 3339 timestamps.
- labels: ordered string collection.
- commit references: ordered full Git commit SHAs.
- dependencies: directed references to other task IDs.

There is no priority field in the MVP.

### Task identity

Use SQLite INTEGER PRIMARY KEY AUTOINCREMENT for new IDs. Imported legacy IDs are inserted explicitly and preserved. Physical deletion is prohibited, so normal operation never frees an ID for reuse.

### Task outcome

Workflow status is configurable; outcome is semantic. A task can be in any configured status, while outcome records whether it ended as completed or cancelled. The outcome is set from the target status configuration when a task enters a mapped status. An explicit task.cancel operation sets cancelled without changing the workflow status and accepts an optional reason. task.complete sets completed without changing the workflow status. task.reopen clears the outcome. If the current status itself maps to an outcome, reopen requires a target_status_id that is enabled, has no terminal outcome, and is reachable through a configured transition. If the current status has no outcome mapping, target_status_id may be omitted and the status is retained. Repeated completion/cancellation or status changes on a terminal task are rejected until reopen. The operation history preserves every previous outcome and cancellation reason.

### Dependencies

Dependencies are directed edges within one project store. Self-dependencies and direct/indirect cycles are rejected. A dependency update and its task version/history changes are one transaction. Dependency state is exposed through the interface; dependency presence alone does not impose a completion gate unless a later product decision explicitly adds one.

## 5. SQLite schema contract

The production schema is defined in versioned SQL migrations. The v1 DDL baseline is documented in docs/architecture/sqlite-schema-v1.sql. The following logical table set is mandatory:

| Table | Required content |
|---|---|
| store_metadata | Schema version, project ID, creation metadata. |
| tasks | Current task fields, stable ID, current status and outcome. |
| task_labels | Ordered labels with a normalized lookup key. |
| task_commit_refs | Ordered full commit SHAs. |
| task_dependencies | Directed edges, unique per pair, foreign-keyed to tasks. |
| task_versions | Immutable per-task version number and complete JSON snapshot. |
| task_baselines | Current user-approved version per task. |
| operation_history | Append-only successful and rejected operation events. |
| task_locks | Current lock owner, token hash, lease timestamps, fencing generation. A released row is retained so the fencing generation never resets. |

Status definitions and transitions remain in TOML, not duplicated as a second writable status database. tasks.status_id is not a foreign key to the config file; this preserves readability of historical/disabled statuses.

### Version snapshots

Each version snapshot is canonical JSON containing all user-visible task state:
- task ID, title, status ID, outcome, body, timestamps;
- labels in display order;
- commit references in display order;
- dependency task IDs in stable sorted order.

The snapshot is immutable. Object keys are serialized deterministically. Version comparison returns field-level changes; it does not mutate either version.

### Operation history

Each event stores:
- event ID and task ID (nullable for store-level events);
- actor ID and UTC timestamp;
- operation name;
- previous value/snapshot and requested/new value/snapshot where applicable;
- outcome (succeeded or rejected);
- stable error code for rejected operations;
- configuration revision for status/config-sensitive operations;
- lock fencing generation when the operation is lock-protected.

Successful mutations append their event in the same transaction as task state and version. Rejected task operations are rolled back and then recorded in a separate transaction. If the database itself cannot accept the rejection event, the caller receives an explicit history_record_failed condition; the system must not claim that the event was recorded.

Database triggers MUST prevent UPDATE and DELETE on task_versions and operation_history. A trigger MUST reject physical deletion from tasks.

### Baseline

task_baselines stores one current approved baseline per task, referencing a version belonging to that same task. Replacing a baseline is an explicit user approval operation and creates an operation-history event. Previous approvals remain visible in history. The CLI approval command requires interactive confirmation. Todoing records actor attribution but does not authenticate a human; Manifest must invoke the JSON approval operation only after an explicit user approval.

## 6. Transaction rules

Connection initialization MUST:
1. Open the database.
2. Enable and verify foreign keys.
3. Apply the configured busy timeout.
4. Verify rollback-journal mode is DELETE.
5. Set/verify synchronous FULL.

### Successful task mutation

Use one IMMEDIATE transaction:
1. Check task and project identity.
2. Check lock owner/token/fencing generation where a task lock is active.
3. Validate status transition and configuration.
4. Validate dependency graph changes.
5. Update task fields and normalized labels/commits/dependencies.
6. Insert the next immutable version.
7. Append the successful operation-history event.
8. Commit.

No partial task/version/history state may be visible.

### Read operation

Use one read transaction. Check lock ownership and retrieve task data within the same transaction. A non-owner cannot read the task, versions, history, labels, commit references, or task-specific metadata while its lock is active.

List/search operations MUST omit all locked tasks; they must not return locked task IDs or fields. The lock owner reads its task through a direct get with a valid token. A non-owner direct read by ID returns task_locked without task content.

### Lock acquisition

Use a short EXCLUSIVE transaction:
1. Wait for all active readers/writers to finish.
2. Verify task exists.
3. Verify there is no unexpired lock (including recovery grace).
4. Create lock record with new fencing generation.
5. Append lock-acquired event.
6. Commit. This commit is the acquisition linearization point.

This temporarily blocks readers of the whole project database. The transaction must contain no slow external work.

### Busy behavior

The busy timeout is 10 seconds. If SQLite remains busy after the timeout, return store_busy with the operation retryable flag. Do not silently retry a mutation after an ambiguous commit result.

## 7. Lock protocol

### Defaults

- Lease: 300 seconds.
- Heartbeat: every 60 seconds.
- Recovery grace: 60 seconds after lease expiry.
- Lease duration MUST be at least 60 seconds. Values may be changed in project configuration, subject to validation: heartbeat MUST be positive and less than one third of lease duration.

The lock record uses UTC epoch milliseconds for acquisition, heartbeat, and expiry. Clock source is abstracted behind a testable clock interface. The system assumes the local machine clock is maintained by the OS time service. A forward clock jump may expire a lease early; a fencing generation prevents the stale owner from writing after recovery. A backward jump may delay recovery. The CLI exposes expiry details and does not automatically steal an expired lock.

### Ownership

- actor_id is attribution.
- owner_id identifies the work session.
- A 256-bit cryptographically random token is returned once on acquisition.
- Only a SHA-256 hash of the token is stored.
- The raw token MUST NOT appear in command-line arguments, logs, or task/history data.
- CLI clients pass it through TODOING_LOCK_TOKEN; JSON API clients include it in the request body.
- The fencing generation increments on every new acquisition after expiry/recovery.

### Operations

- acquire: fails with task_locked if an unexpired lock exists.
- heartbeat: renews only when owner ID, token, and fencing generation match.
- release: removes the lock only for the current owner; records a history event.
- recover: succeeds only after lease expiry plus recovery grace; atomically replaces owner, increments fencing generation, and records recovery.
- No automatic takeover and no force-unlock command in the MVP.

Every read and mutation path must enforce the lock. There is no bypass for human CLI commands. The lock owner can read and mutate its task; other clients cannot read task content. The list command hides tasks locked by another client.

## 8. Actor identity

Every operation-history event requires an actor_id.

- JSON API requests MUST supply actor_id.
- Human CLI commands default to user:<OS account name> if no explicit actor is supplied.
- Agent CLI calls MUST supply an explicit stable actor ID using --actor or TODOING_ACTOR.
- Actor IDs are opaque, non-empty strings with a documented maximum length.
- Actor identity is attribution, not authentication. The lock token is the ownership capability.

The lock owner_id is a separate session identifier and may be different from the actor ID.

## 9. Versioned JSON API v1

The API is a one-request-per-process interface. The todoing api command reads one JSON object from stdin and writes exactly one JSON response to stdout. It does not run a server and does not use JSON Lines streaming.

### Request envelope

~~~json
{
  "api_version": 1,
  "request_id": "optional-correlation-id",
  "project_id": "project-identifier",
  "actor_id": "agent:manifest-executor",
  "operation": "task.get",
  "args": {
    "task_id": 42
  },
  "lock_token": null
}
~~~

- api_version is required and must equal 1.
- request_id is optional and is echoed when supplied.
- project_id is required and must match store metadata.
- actor_id is required.
- operation is a stable operation identifier.
- args is operation-specific and validated strictly.
- lock_token is supplied only for operations on a locked task.

### Success response

~~~json
{
  "api_version": 1,
  "request_id": "optional-correlation-id",
  "ok": true,
  "data": {}
}
~~~

### Error response

~~~json
{
  "api_version": 1,
  "request_id": "optional-correlation-id",
  "ok": false,
  "error": {
    "code": "task_locked",
    "message": "Task is locked by another client",
    "retryable": false,
    "details": {}
  }
}
~~~

Errors are JSON responses on stdout. Human diagnostics, if any, go to stderr. No non-JSON text may be mixed into API stdout.

### Initial operation namespace

- store.info, store.verify, store.migrate, store.snapshot
- task.create, task.list, task.get, task.update, task.status.set, task.complete, task.cancel, task.reopen
- task.label.add, task.label.remove
- task.commit.add
- task.dependency.add, task.dependency.remove, task.dependency.list
- task.history.list
- task.version.list, task.version.get, task.version.compare, task.version.approve
- task.lock.acquire, task.lock.heartbeat, task.lock.release, task.lock.recover

The request and response envelopes are specified in docs/api/v1/request.schema.json and docs/api/v1/response.schema.json. Operation arguments, results, lock semantics, and no-op behavior are specified in docs/api/v1/operations.md. Per-operation JSON Schema definitions are added before their handlers are implemented. Unknown operations and unsupported envelope fields are rejected, not silently ignored.

### Error codes

| Code | Meaning |
|---|---|
| invalid_request | Invalid JSON envelope or operation arguments. |
| unsupported_api_version | API version is not supported. |
| project_id_mismatch | Request project ID differs from store metadata. |
| config_invalid | Status configuration is invalid. |
| task_not_found | Task ID does not exist. |
| task_terminal | Task already has a completed/cancelled outcome and must be reopened first. |
| task_locked | Task is locked by another client. |
| lock_expired | Lock lease expired. |
| stale_lock | Owner/token/fencing generation is no longer valid. |
| invalid_status_transition | Transition is not allowed by project config. |
| dependency_not_found | Referenced dependency does not exist. |
| dependency_cycle | Change would introduce a cycle. |
| version_not_found | Requested version does not exist. |
| store_busy | SQLite remained busy past the configured timeout. |
| store_corrupt | Store integrity or schema validation failed. |
| migration_conflict | Legacy migration cannot proceed without user resolution. |
| io_error | Filesystem or persistence operation failed. |
| history_record_failed | Rejected operation could not be appended to history. |
| internal_error | Unexpected failure; no implementation details leaked. |

### Process exit codes

| Exit code | Meaning |
|---:|---|
| 0 | Request succeeded. |
| 2 | Invalid CLI usage, JSON, configuration, or request. |
| 3 | Requested task/version/dependency not found. |
| 4 | Domain or concurrency conflict (lock, stale owner, invalid transition, dependency cycle). |
| 5 | Storage, integrity, or filesystem failure. |
| 6 | Migration failure/conflict. |
| 70 | Unexpected internal error. |

A JSON API error response is still emitted on stdout for non-zero exit codes.

## 10. Migration procedure

Migration is explicit and non-destructive.

1. Refuse to run if a target database already exists unless the user invokes a separately specified recovery/inspection command.
2. Read Markdown task files as source records; rebuild or ignore a corrupt/missing JSON index.
3. Validate IDs, titles, timestamps, statuses, labels, commit refs, and task bodies.
4. Generate the legacy-compatible TOML config if none exists; do not overwrite a user-supplied config.
5. Build a staging SQLite database under .todoing/migration/.
6. Insert tasks with original numeric IDs and all supported fields.
7. Create version 1 and a migration_import history event for each imported task. Do not invent unavailable edit history.
8. Verify row counts, IDs, task snapshots, labels, commit refs, dependencies (none in legacy format), and PRAGMA integrity_check.
9. Close every staging connection.
10. Activate the database using a same-volume rename after confirming no target exists.
11. Write a migration report with counts, warnings, and source file hashes.
12. Retain original Markdown files and index unchanged.

On any failure before activation, the staging database is not made active and the source remains usable. On Windows, activation must retry only known transient sharing violations and must never delete/overwrite the source.

## 11. Backup snapshot

Todoing exposes store.snapshot using SQLite's Online Backup API through rusqlite's backup support. It creates a consistent standalone database snapshot and is rejected while any task lock is active, preventing backup from bypassing task read exclusion. It does not choose destination, retention, or remote replication; those remain Manifest/project responsibilities.

A snapshot is reported successful only after the destination database is reopened and passes integrity verification. Raw copying of a live database file is prohibited as a supported backup method.

## 12. CLI compatibility

Keep existing human commands where the semantics still make sense:
- add, ls, see, body, append, status, label, commit.

Add explicit complete, cancel, and reopen operations. Cancellation reason is optional; it is stored in the operation-history event.

Add:
- depends, history, version, lock, config, migrate, verify, snapshot, and api.

Compatibility changes:
- delete remains recognized for a transition period but always rejects physical deletion with a clear error.
- edit opens a temporary Markdown representation, validates it, and saves through the transactional update path; it never edits the database source directly.
- ls --scan is deprecated after migration; verify checks database integrity.
- Current bundled hooks are legacy behavior. Manifest-specific completion hooks are removed from the target core; Manifest owns its OpenSpec/Git checks. Generic lifecycle events/hooks are post-MVP.
- --json human CLI output uses the same versioned response envelope as API v1.

## 13. Implementation dependencies

Production dependencies should remain minimal:
- rusqlite with bundled and backup features.
- serde and serde_json.
- toml with Serde support.
- clap.
- thiserror or equivalent typed error support.
- A cryptographically secure random source and SHA-256 for lock tokens.
- Existing time/date library where possible; timestamps are normalized to UTC RFC 3339 at the domain boundary.

No async runtime, HTTP framework, ORM, or external database service is required.

## 14. Implementation order

1. Add domain types, actor context, error taxonomy, and configuration parser with tests.
2. Add SQLite connection factory, schema migrations, and integrity checks.
3. Add transactional task repository and ID allocation.
4. Add immutable versions, baselines, and append-only operation history.
5. Add configurable statuses/outcomes and transition validation.
6. Add dependencies and cycle detection.
7. Add lock lease, EXCLUSIVE acquisition barrier, heartbeat, release, recovery, and fencing.
8. Add CLI commands and JSON API v1 schemas/contract tests.
9. Add non-destructive migration and snapshot backup.
10. Verify cross-platform tests and update current-behavior specs only after implementation is actually present and tested.


## 12. Private project data and Git boundary

The `.todoing/` directory is private, project-local runtime data. It MUST be excluded from Git in every consuming project. It contains the SQLite database, user-edited `config.toml`, legacy task files and index, migration reports, and snapshots. None of these files may be committed, pushed, or replicated through the application repository.

Before `init` creates the directory or `migrate` reads or writes it, Todoing checks whether the project root is inside a Git worktree. If so, it refuses to continue when any `.todoing/` path is already tracked, ensures the project-root `.gitignore` contains `/.todoing/`, and verifies the ignore rule with Git. It never automatically removes tracked files from the index or rewrites history. The user must remove already-tracked private files from the index and review any published history before retrying.

Git ignore rules prevent ordinary accidental staging; they are not a security boundary against deliberate `git add -f`. Consumers must never force-add private data. Todoing does not synchronize task data to GitHub or any online database.
