# Todoing JSON API v1 — Operation Contract

**Status:** Normative operation-level contract for MVP implementation.
**Envelope schemas:** request.schema.json and response.schema.json.
**Transport:** One JSON request read from stdin and one JSON response written to stdout by one todoing api process.

## 1. Common rules

- Every request includes api_version, project_id, actor_id, operation, and args.
- request_id is optional and is echoed unchanged when supplied.
- project_id must match the current store metadata.
- actor_id is required for reads and writes. It is attribution, not authentication.
- Unknown operation names, unknown envelope properties, invalid types, and unsupported API versions are rejected.
- Successful responses use the v1 success envelope. Failed operations use the v1 error envelope.
- JSON responses, including errors, are written to stdout. Diagnostics are written to stderr.
- A non-zero process exit code accompanies an error response.
- If a task has an active lock owned by another client, reads of that task are rejected. List/search operations omit locked tasks. Task-specific history, versions, dependencies, labels, commit references, external references, and Review history are also protected.
- Task locks are opt-in. When a task has an active lock, every task-specific read or mutation is restricted to its owner; the owner supplies a valid token in the request envelope's lock_token field. When no active lock exists, operations do not require a token. The raw token is never accepted in command-line arguments.
- Every successful task-state save creates a new immutable task version and a successful operation-history event in the same transaction.
- A rejected task operation that reaches the application operation layer is recorded in operation history after the attempted mutation is rolled back. If the store cannot accept the rejection event, the response reports history_record_failed.
- Lock acquisition/heartbeat/release/recovery and baseline approval append history events but do not create task versions unless task state also changes.

## 2. Store operations

| Operation | Required args | Optional args | Result |
|---|---|---|---|
| store.init | none | none | project_id, schema_version, API version; initializes a new store using the envelope project_id. |
| store.info | none | none | project_id, schema_version, API version, task count. |
| store.verify | none | deep (boolean, default true) | integrity status and structured diagnostics. |
| store.migrate | none | dry_run (boolean, default false) | migration report, imported count, warnings; requires legacy store and no active target DB. |
| store.snapshot | output_path (non-empty string) | overwrite (boolean, default false) | verified snapshot path, byte size, checksum. |
| store.snapshot.verify | snapshot_path (non-empty string) | expected_sha256 | snapshot integrity, project/schema compatibility, byte size and SHA-256. |
| store.snapshot.restore | snapshot_path (non-empty string) | expected_sha256 | restored integrity report and retained pre-restore safety snapshot. |

`store.snapshot` requires `output_path` as a non-empty string identifying the destination file. `overwrite` is a boolean and defaults to false; an existing destination is preserved unless it is explicitly set to true.

store.init creates a new project-local store only when no database exists. It validates and preserves an existing config.toml, or creates the generic default configuration when absent. It refuses to initialize over legacy Markdown task files or index.json; use store.migrate for those projects. A repeated or concurrent initialization that loses the no-clobber database activation returns store_already_initialized. The CLI may generate a project ID when --project-id is omitted; JSON API callers must supply project_id in the request envelope.

store.migrate is non-destructive. It refuses to overwrite an existing database. A dry run validates sources and reports expected actions without activating a database.

store.snapshot uses SQLite Online Backup API. It is rejected while any task lock is active so a client cannot use a snapshot to bypass task read exclusion. The destination must not resolve to the active database path. Existing destination files are not overwritten unless overwrite=true. The snapshot is reopened and integrity-checked before success is returned.

store.snapshot.verify opens the supplied snapshot independently and checks project identity, the exact supported schema, SQLite integrity, foreign keys and SHA-256. Relative paths are resolved from the project root. When expected_sha256 is supplied, it must match the computed digest.

store.snapshot.restore restores the complete store from a verified snapshot using SQLite Online Backup API into the existing database connection; it does not replace the database file. The operation rejects the active database as its source, rejects active task locks, and takes an exclusive SQLite database lock while checking and restoring. Before restore it creates a verified safety snapshot under .todoing/backups/. The safety snapshot is retained after success. If restore or post-restore verification fails, Todoing attempts to restore the safety snapshot before returning the original error. Supplying expected_sha256 is recommended and rejects a mismatched snapshot before any mutation with `snapshot_checksum_mismatch`. Restore requires a snapshot with the same project ID and currently supported schema version; automatic downgrade/upgrade is not performed. All Todoing processes must allow the exclusive database lock to be acquired; open readers/writers are blocked by SQLite during restore.

## 3. Task operations

### task.create

Required:
- title: non-empty string.

Optional:
- status_id: enabled status ID; defaults to the first enabled status in configuration.
- body: string; defaults to empty.
- labels: array of non-empty strings; defaults to empty.

Result: the created task, version 1, and event ID. The ID is allocated transactionally and never reused.

### task.list

Optional:
- status_id: exact status ID filter.
- outcome: active, completed, or cancelled.
- labels: array of labels; multiple labels use AND semantics and case-insensitive comparison.
- query: case-insensitive substring across title, body, and labels; exact match against status ID.
- limit: positive integer; omitted means no limit.
- reverse: boolean; default false. Default order is descending task ID; reverse=true is ascending.
- archive_state: active (default), archived, or all.

All locked tasks are omitted entirely from list/search results, including their IDs and fields. The lock owner reads its task through task.get with its token.

### task.get

Required:
- task_id: positive integer.

Result: current task fields, labels, commit references, dependency IDs, active external references, current outcome, current version number, baseline version number if one exists, revision_count, and final_commit_sha.

A locked task is readable only by its lock owner with a valid lock token. Archived tasks remain readable by ID when not locked, but are omitted from the default task.list result.

### task.archive

Required:
- task_id.

Moves a task into the archive without changing workflow status or terminal outcome. The task and all its history, versions, dependencies, labels, and commit references are retained. The operation is idempotent; an already archived task returns no_op=true without creating a task version.

### task.restore

Required:
- task_id.

Restores an archived task to the active list without changing workflow status or terminal outcome. The operation is idempotent; an already active task returns no_op=true without creating a task version.

Archived tasks are read-only until restored. Ordinary mutations and new lock acquisition are rejected with task_archived. Archive and restore obey existing lock ownership rules. Archiving does not cascade to dependent or prerequisite tasks.

### task.update

Required:
- task_id.
- patch: object containing one or more of title and body.

Optional fields not present in patch are unchanged. Empty patch is rejected. Status, labels, commits, dependencies, and outcome are changed through their dedicated operations.

Result: updated task and new version.

### task.status.set

Required:
- task_id.
- status_id: enabled configured status.

The transition must be explicitly permitted by config.toml. A task with a non-null outcome must be reopened before status changes. Entering a status mapped to an outcome sets that outcome.

Result: updated task and new version.

### task.complete

Required:
- task_id.

Sets outcome=completed without changing workflow status. Rejects a task that already has a terminal outcome.

### task.cancel

Required:
- task_id.

Optional:
- reason: string; optional and may be empty/omitted.

Sets outcome=cancelled without changing workflow status. The reason is recorded in operation-history details. Rejects a task that already has a terminal outcome.

### task.reopen

Required:
- task_id.

Optional:
- target_status_id: enabled configured status.

Clears the terminal outcome. If the current status maps to an outcome, target_status_id is required and must be a permitted transition to a status without a terminal outcome. If the current status has no outcome mapping, target_status_id may be omitted and the current status is retained.

Result: updated task and new version.

## 4. Labels and Git references

### task.label.add / task.label.remove

Required:
- task_id.
- label: non-empty string.

The operation is idempotent. Label matching for duplicate prevention is case-insensitive; the original display spelling is preserved for the first stored label.

Result: updated task and new version when state changes. A no-op does not create a new version.

### task.commit.add

Required:
- task_id.
- revision: Git revision expression resolvable in the project repository.

Todoing resolves the revision to a full commit SHA and stores the SHA. Commit references are traceability metadata, not proof of completion.

Result: updated task and new version when a new reference is added. Re-adding an existing SHA is a no-op.

## 5. External references, Review history, and final commit

### task.external-ref.add

Required:
- task_id
- namespace: non-empty string, up to 128 characters.
- value: non-empty string, up to 512 characters.
- relation: non-empty string, up to 128 characters.

Optional:
- scope: task (default) or project.

Honors task lock ownership; when the task is locked, the caller must supply its valid lock token. Active namespace/value pairs are unique within a task for task scope and across the project for project scope. An identical active reference on the same task is a no-op. Reusing an active pair with different relation/scope on the same task returns external_ref_conflict. A project-scoped pair conflicts with any active reference using that pair anywhere in the project; task-scoped pairs may be reused on different tasks. Successful changes create a version and history event atomically.

### task.external-ref.list

Required:
- task_id.

Optional:
- include_removed: boolean, default false.

Honors task read exclusion; when the task is locked, only its owner with a valid lock token may read. Returns references ordered by creation time. Removed references are included only when include_removed=true.

### task.external-ref.remove

Required:
- task_id
- reference_id

Honors active task-lock ownership: an unlocked task may be modified without a token; when locked, only the owner with a valid token may remove the reference. Removal is soft: removed_at and removed_by are retained. Repeated removal is a no-op; an unknown reference_id returns external_ref_not_found. A successful removal creates a task version and history event atomically.

### task.review.add

Required:
- task_id
- review_id: stable non-empty ID, up to 128 characters.
- version_no: existing version number of the same task.
- reviewer: non-empty reviewer label, up to 256 characters.
- outcome: approved or changes_requested.

Optional:
- comments: ordered JSON array of JSON values (any valid JSON value is allowed as an array element), default empty.

Honors active task-lock ownership: an unlocked task may be modified without a token; when locked, only the owner with a valid token may add a Review. Review records are append-only and linked to the reviewed immutable version. Repeating review_id with the same canonical payload is a no-op; a different payload returns review_id_conflict. A missing version returns review_version_not_found. Each newly accepted changes_requested review increments revision_count exactly once. Approval does not change workflow status or revision_count. Record, counter, task version, and history event are committed atomically. Result: the Review record, reviewed_version_no, task_version_no, revision_count, and no_op.

### task.review.list

Required:
- task_id.

Optional:
- before_record_id: pagination cursor; returns records with smaller IDs.
- limit: positive integer; default 100.

Requires read access under the task lock. Returns immutable review records newest-first, including record_id for pagination.

### task.commit.finalize

Required:
- task_id
- revision: Git revision expression resolvable to a commit in the project repository.

Honors active task-lock ownership: an unlocked task may be modified without a token; when locked, only the owner with a valid token may set the marker. Todoing resolves the revision to a full Git SHA. Repeating the current SHA is a no-op. Replacing an existing SHA requires the task to have been explicitly reopened after the previous assignment; otherwise task_terminal is returned. A terminal task must be reopened before changing the marker. The prior SHA remains in immutable task versions and operation history. The marker cannot be cleared. Result: task_id, resolved final_commit_sha, task_version_no, and no_op.

## 6. Dependencies

### task.dependency.add

Required:
- task_id.
- depends_on_task_id.

Both tasks must exist in the same project store. Self-dependencies and direct/indirect cycles are rejected. The cycle check and edge insert occur in one transaction.

Result: updated task, new version, and event ID.

### task.dependency.remove

Required:
- task_id.
- depends_on_task_id.

Result: updated task and new version when an edge is removed. Removing a missing edge returns dependency_not_found.

### task.dependency.list

Required:
- task_id.

Returns dependency IDs and a parallel `states` array containing each dependency's status, outcome, archive state, and lock state. A dependency task locked by another client is represented without protected task fields; its status, outcome, and archive state are null and `locked` is true.

## 7. History and versions

### task.history.list

Required:
- task_id.

Optional:
- before_event_id: pagination cursor; returns events with smaller IDs.
- limit: positive integer; default 100.

Returns newest-first operation events with actor, timestamp, operation, previous/requested values, outcome, and error code where applicable.

### task.version.list

Required:
- task_id.

Optional:
- before_version_no: pagination cursor.
- limit: positive integer; default 100.

Returns immutable version metadata, newest first.

### task.version.get

Required:
- task_id.
- version_no.

Returns the complete immutable task snapshot.

### task.version.compare

Required:
- task_id.
- from_version_no.
- to_version_no.

Returns deterministic field-level differences between two versions of the same task.

### task.version.approve

Required:
- task_id.
- version_no.

Sets the user-approved baseline to an existing version belonging to that task. Appends an operation-history event. Does not create a task version. The human CLI command requires interactive confirmation. Todoing does not authenticate a human actor; Manifest MUST call this JSON operation only after explicit user approval.

## 8. Task locks

### task.lock.acquire

Required:
- task_id.

The caller supplies actor_id in the envelope. Todoing generates an owner/session ID and a 256-bit random lock token. The token is returned once and must be kept secret by the client. Only its SHA-256 hash is stored.

Returns owner_id, lock_token, fencing_generation, acquired_at, heartbeat_at, and expires_at.

Acquisition uses a short SQLite EXCLUSIVE transaction. An active unexpired lock causes task_locked. An expired lock continues to block a new acquire and returns task_locked. The caller must invoke task.lock.recover explicitly after the configured recovery grace; acquire never performs implicit takeover.

### task.lock.heartbeat

Required:
- task_id.
- owner_id.
- fencing_generation.
- lock_token in the request envelope.

Renews the lease only if owner, token, generation, and lease state all match. An expired lease cannot be renewed.

Returns the new heartbeat_at and expires_at.

### task.lock.release

Required:
- task_id.
- owner_id.
- fencing_generation.
- lock_token in the request envelope.

Releases only the current owner's lock. The lock row is retained with state=released so the fencing generation is never reset.

Returns released_at and fencing_generation.

### task.lock.recover

Required:
- task_id.

No lock token is accepted. Recovery succeeds only after lease expiry plus the configured recovery grace. Todoing creates a new owner/session ID and token, increments fencing_generation, and records the recovery event.

Returns the new owner_id, lock_token, fencing_generation, and expires_at. There is no automatic takeover and no force-recovery operation in the MVP.

## 9. No-op semantics

A successful request that does not change task state (for example, adding an already-present label or commit reference) returns the current task and no_op=true. The accepted operation is recorded in operation history, but no task version is created because no task state was saved.

Lock heartbeat is not a task save. Lock events are recorded in operation history but do not create task versions.

## 10. Contract test requirements

For every operation, tests must cover:
- Valid request and exact response schema.
- Missing/invalid required fields.
- Unknown fields and unsupported operation.
- Project ID mismatch.
- Actor attribution.
- Lock owner and non-owner behavior.
- Stable error code and process exit code.
- stdout contains exactly one JSON response; stderr contains diagnostics only.
- Atomic state/version/history behavior for mutations.
- No-op behavior where applicable.
