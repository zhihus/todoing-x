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
- If a task has an active lock owned by another client, reads of that task are rejected. List/search operations omit locked tasks. Task-specific history, versions, dependencies, labels, and commit references are also protected.
- A valid lock token is supplied in the request envelope's lock_token field for operations on a task locked by the caller. The raw token is never accepted in command-line arguments.
- Every successful task-state save creates a new immutable task version and a successful operation-history event in the same transaction.
- A rejected task operation that reaches the application operation layer is recorded in operation history after the attempted mutation is rolled back. If the store cannot accept the rejection event, the response reports history_record_failed.
- Lock acquisition/heartbeat/release/recovery and baseline approval append history events but do not create task versions unless task state also changes.

## 2. Store operations

| Operation | Required args | Optional args | Result |
|---|---|---|---|
| store.info | none | none | project_id, schema_version, API version, task count. |
| store.verify | none | deep (boolean, default true) | integrity status and structured diagnostics. |
| store.migrate | none | dry_run (boolean, default false) | migration report, imported count, warnings; requires legacy store and no active target DB. |
| store.snapshot | output_path | overwrite (boolean, default false) | verified snapshot path, byte size, checksum. |

store.migrate is non-destructive. It refuses to overwrite an existing database. A dry run validates sources and reports expected actions without activating a database.

store.snapshot uses SQLite Online Backup API. The destination must not resolve to the active database path. Existing destination files are not overwritten unless overwrite=true. The snapshot is reopened and integrity-checked before success is returned.

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

Locked tasks owned by another client are omitted entirely, including their IDs and fields.

### task.get

Required:
- task_id: positive integer.

Result: current task fields, labels, commit references, dependency IDs, current outcome, current version number, and baseline version number if one exists.

A locked task is readable only by its lock owner with a valid lock token.

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

## 5. Dependencies

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

Returns dependency IDs and their current dependency state. A dependency task locked by another client is represented only as a locked reference; its task content is not disclosed.

## 6. History and versions

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

Sets the user-approved baseline to an existing version belonging to that task. Appends an operation-history event. Does not create a task version.

## 7. Task locks

### task.lock.acquire

Required:
- task_id.

The caller supplies actor_id in the envelope. Todoing generates an owner/session ID and a 256-bit random lock token. The token is returned once and must be kept secret by the client. Only its SHA-256 hash is stored.

Returns owner_id, lock_token, fencing_generation, acquired_at, heartbeat_at, and expires_at.

Acquisition uses a short SQLite EXCLUSIVE transaction. An active lock or a lease still within its recovery grace causes a conflict.

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

## 8. No-op semantics

A successful request that does not change task state (for example, adding an already-present label or commit reference) returns the current task and a no-op indicator. It does not create a new task version. Whether no-op attempts are separately recorded in operation history is implementation-defined but must not be reported as a task save.

Lock heartbeat is not a task save. Lock events are recorded in operation history but do not create task versions.

## 9. Contract test requirements

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
