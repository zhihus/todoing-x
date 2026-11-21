# MVP task core — architecture design draft

**Status:** Proposed architecture; technical decisions remain open  
**Related research:** `docs/architecture/research/storage-engine-evaluation.md`  
**Normative requirements:** Capability deltas under this change

## 1. Architecture shape

Use a layered, modular Rust application with a single primary CLI executable.

1. **CLI adapter** — argument parsing, human-readable output, JSON serialization, exit-code mapping.
2. **Application services** — task use cases and transaction orchestration. Every public operation passes through this layer.
3. **Domain model** — task identity, configured workflow, outcomes, dependencies, versions, history, and lock semantics.
4. **Storage adapter** — transaction interface and persistence implementation. The domain and application layers must not depend directly on database-specific APIs.
5. **Migration adapter** — reads the existing Markdown/YAML files and JSON index, validates them, and imports records without modifying the source.

Todoing remains a local-first project tool. The architecture does not introduce a central service, mandatory GUI, Manifest dependency, or harness-specific behavior.

## 2. Logical data model

The persistence model must represent the following logical entities. This is a domain-level inventory, not a final SQL schema or key layout.

| Entity | Responsibility |
|---|---|
| Project metadata | Store format/schema version and project-scoped metadata. |
| Task | Stable ID, title, content, configured workflow status, distinct completion/cancellation outcome, timestamps, and supported metadata. |
| Status definition | User-configured status identity, display label/order, and availability for new transitions. |
| Status transition rule | Allowed source-to-target workflow transitions. |
| Dependency | Directed relationship between two task IDs. |
| Task version | Immutable snapshot of a task state after a successful save. |
| Operation history | Append-only actor, timestamp, operation, previous value, new value, and outcome. |
| Approval/baseline marker | Identifies the task version approved by the user. |
| Task lock | Owner identity, lease/expiry information, and heartbeat state. |

Physical task deletion is not part of the domain API. Task IDs are stable and never reused.

Git commit references remain traceability metadata. They are not proof of task completion and do not replace task history or versions.

## 3. Application transaction boundary

A task mutation is one application-level unit of work.

The commit boundary includes, as applicable:

- the new task state;
- a new immutable task version;
- the corresponding append-only history entry;
- dependency changes;
- indexes and derived state;
- any state needed to preserve identifier uniqueness.

A mutation must not report success until the complete unit of work is committed. If any part fails, no partial result may become visible as committed state.

The storage adapter must expose explicit transaction semantics. It must not expose a sequence of independent file writes as if they were one transaction.

## 4. Configurable workflow

Status definitions and transition rules are project configuration, not hard-coded Rust enums.

At startup or project-store opening, the application validates the configuration before accepting mutations. The configuration model must support:

- stable status keys;
- user-facing labels;
- user-controlled ordering;
- enabling/disabling a status for new transitions;
- permitted source-to-target transitions.

Historical task records remain readable when a status is disabled or retired.

Completion and cancellation are distinct outcomes. They must not be collapsed into ordinary workflow status values. The exact mapping between workflow status and outcome is still an integration design decision.

**Decision:** use TOML at `.todoing/config.toml` for user-edited status and transition configuration. The exact field schema, schema versioning, defaults for a new project, and behavior when a referenced status definition is missing remain to be specified. See `docs/architecture/research/status-config-format.md`.

## 5. Dependencies

Dependencies are directed edges between stable task IDs.

The application service validates that both tasks exist and that a proposed edge does not create a cycle. Dependency state is exposed through the public interface. The exact derived fields and any rule that blocks work based on unfinished dependencies remain open.

Cycle validation and edge persistence must occur in the same transaction to prevent concurrent changes from creating a cycle between validation and commit.

## 6. Versions and history

Every successful save creates a new immutable task version. The version captures enough task state to inspect and compare versions without relying on Git history or the current task row.

Every successful operation appends an immutable history record. The history records actor, timestamp, operation, before/after values, and outcome. A correction is another operation; existing entries are never edited.

The user-approved baseline points to a specific version. Approval must not mutate or rewrite the version snapshot.

**Open:** actor identity source, snapshot serialization, diff semantics, retention policy, and whether approval history is represented as a separate event or a version annotation.

## 7. Task locks — critical architecture constraint

Task work-session locks are an application-level access policy, not a side effect of the database's ordinary read/write transaction locks.

The required behavior is strict: while one owner holds a task lock, other clients cannot read or mutate that task. The owner remains authorized. The protocol must support owner identity, heartbeat/lease renewal, explicit release, timeout, and abandoned-lock recovery.

Every public task access path must enforce the lock policy, including reads used by listing, filtering, dependency inspection, history/version inspection, and migration/maintenance operations where applicable.

The storage-engine comparison shows why database concurrency alone is insufficient: SQLite WAL and redb MVCC allow readers to coexist with writers. The design must therefore address:

- whether lock acquisition waits for already-running task reads to finish;
- how a new read is prevented from racing with lock acquisition;
- how a client proves that it is the lock owner on later CLI invocations;
- how stale reader/owner state is recovered after process termination;
- how lock acquisition, renewal, expiry, and task mutation are serialized across OS processes.

The selected coordination model is a hybrid protocol: persist the long-lived owner lease in the transactional store, and use short-lived per-task shared/exclusive OS file locks as an access barrier. Ordinary task operations hold a shared barrier while checking the lease and accessing the task. Lock acquisition, release, and stale-lease recovery take the exclusive barrier, which drains in-flight operations. The task-lock research prototype verified the shared/exclusive OS barrier on Windows and Ubuntu; the persisted lease, owner authorization, and recovery logic still require implementation tests. See `docs/architecture/research/task-lock-protocol.md`.

A task lock must not be implemented by holding a long-lived database transaction for the entire agent work session.

## 8. Storage engine boundary

The application layer depends on a storage interface that can express atomic units of work, uniqueness constraints, and concurrent access results.

**Decision:** use SQLite via rusqlite with the bundled SQLite library. Use WAL mode on a local project filesystem; network filesystems are unsupported. The isolated prototype passed transaction rollback, process-exit recovery, and multi-process writer tests on Windows and Ubuntu. SQLite was selected over redb because its relational model reduces custom indexing and consistency code for tasks, dependencies, versions, history, statuses, and locks. See `docs/architecture/research/storage-engine-evaluation.md`.

The production schema, SQL migrations, busy handling, and backup/restore procedure remain to be specified.

## 9. CLI and JSON contract

The CLI remains the primary interface. Human-readable output and machine-readable JSON are two renderings of the same application results.

The JSON contract must have an explicit version and deterministic response structures. Errors use stable machine-readable codes and predictable process exit codes; diagnostic text is kept off JSON stdout.

The command surface must cover:

- task creation, inspection, and mutation;
- configured status and transition inspection;
- dependency operations and dependency state;
- history and version inspection/comparison;
- approval/baseline operations;
- lock acquisition, renewal, release, and inspection.

Exact command names, argument forms, JSON envelope, error-code registry, and version-negotiation mechanism remain open.

## 10. Migration

Migration is a separate application use case, not an incidental side effect of opening the new store.

It reads the current task Markdown/YAML records and JSON index, validates source data, and imports supported data while preserving original task IDs, timestamps, status values, labels, bodies, and Git commit references.

The source files remain unchanged. Invalid or unsupported records are reported with enough context for resolution; a partial import must not be reported as complete success.

The exact conflict policy, rerun behavior, treatment of unknown frontmatter fields, and migration report format remain open.

## 11. Cross-platform and failure model

Windows and Unix-like systems are supported targets. The implementation and tests must cover separate OS processes, not only threads within one process.

The design must explicitly account for:

- process termination during a mutation;
- OS crash or power loss during commit;
- simultaneous task creation and conflicting updates;
- lock owner termination and lease expiry;
- database open/close and backup/restore;
- file-lock behavior and unsupported network filesystems;
- schema upgrades and migration interruption.

The application must return deterministic errors for recoverable conflicts and must never silently discard task data.

## 12. Decisions required before implementation

The following details remain open and must be specified before their corresponding implementation begins:

1. Task lock owner identity versus secret credential, credential delivery, lease duration, heartbeat interval, expiry/recovery policy, and list/filter behavior for locked tasks.
2. Actor identity source and the default actor when the caller does not provide one.
3. Exact completion/cancellation outcome mapping for configurable workflow statuses.
4. JSON contract envelope, error-code registry, and process exit-code mapping.
5. Migration conflict, rerun, and unknown-frontmatter policy.
6. Exact schema fields for structured task metadata and future attachment support.
7. Backup/restore procedure and integrity verification.

The storage engine (SQLite), project configuration format (TOML), and lock-coordination model (persisted lease plus OS access barrier) are selected. Their implementation details must follow the decisions below and must not be silently changed.


## 13. Project configuration schema

The project workflow configuration is stored at `.todoing/config.toml`.

The initial schema is:

```toml
schema_version = 1
default_status = "todo"

[[statuses]]
id = "todo"
label = "To do"
order = 10
enabled = true

[[statuses]]
id = "in_progress"
label = "In progress"
order = 20
enabled = true

[[statuses]]
id = "done"
label = "Done"
order = 30
enabled = true
outcome = "completed"

[[statuses]]
id = "cancelled"
label = "Cancelled"
order = 40
enabled = true
outcome = "cancelled"

[[transitions]]
from = "todo"
to = "in_progress"

[[transitions]]
from = "in_progress"
to = "done"

[[transitions]]
from = "in_progress"
to = "cancelled"
```

Validation rules:

- `schema_version` is required and must be supported.
- Status IDs are unique, non-empty stable keys.
- Display labels and ordering are user-controlled.
- The default status must exist, be enabled, and have no terminal outcome.
- Every transition endpoint must reference a defined status.
- A disabled status cannot be a target for a new transition. A task already using a disabled status remains readable and may transition out if an explicit transition rule permits it.
- A status may map to no outcome, `completed`, or `cancelled`.
- The task stores its current outcome separately from its workflow status. The outcome is set when entering a status with an outcome mapping and cleared when moving to a status without one. Configuration edits do not retroactively rewrite existing task outcomes.
- Duplicate transitions and self-transitions are rejected.

This initial schema is a design baseline; acceptance tests must be written before implementation.

## 14. SQLite persistence model

The project database path is `.todoing/todoing.db`.

The logical schema consists of:

| Table | Key fields and purpose |
|---|---|
| `project_meta` | Schema version and project-scoped metadata. |
| `tasks` | `id INTEGER PRIMARY KEY AUTOINCREMENT`, title, status key, outcome, body, timestamps, metadata JSON. |
| `task_labels` | Task ID and normalized label; indexed for label filtering. |
| `task_git_commits` | Task ID and resolved full Git SHA. |
| `task_dependencies` | Task ID and dependency task ID; unique directed edge with foreign keys. |
| `task_versions` | Immutable version ID, task ID, per-task version number, full JSON snapshot, timestamp, actor. |
| `operation_history` | Immutable event ID, task ID, actor, timestamp, operation, before JSON, after JSON, result. |
| `task_version_approvals` | Current approved baseline version per task, approver, and timestamp. Approval changes are also recorded in operation history. |
| `task_locks` | Task ID, owner ID, token hash, acquisition time, heartbeat time, and lease expiry. |

Database rules:

- Foreign keys are enabled on every connection.
- WAL mode is used on local filesystems only.
- Write operations use an immediate transaction so validation, ID allocation, mutation, version, history, dependencies, and derived indexes share one serialization boundary.
- The default busy timeout is 30 seconds; a timeout returns a stable conflict/storage error rather than silently dropping a mutation.
- Task IDs are never physically deleted or reused.
- Version and history rows are append-only. Database triggers should reject update/delete attempts on these tables.
- Schema migrations are versioned through `PRAGMA user_version` and applied transactionally.
- The database is not supported on network filesystems. All processes accessing one project store must run on the same host.

The application layer uses a storage interface; SQL statements and transaction handles remain inside the SQLite adapter.

## 15. Version and history representation

A task version is a complete snapshot of the task state after a successful save, serialized as deterministic JSON. It includes the task fields and supported metadata, but not ephemeral lock credentials.

An operation history entry records:

- actor identifier;
- UTC timestamp;
- operation name;
- previous task snapshot or null for creation;
- new task snapshot or null for a non-state operation;
- result/outcome.

Every successful task save creates exactly one new version and one corresponding history entry in the same SQLite transaction. Failed mutations create neither. No-op requests that do not change task state do not create a version.

The approved baseline points to a specific version ID. Re-approval moves the pointer and appends a history entry; it does not alter old versions.

## 16. Task lock protocol

The selected protocol uses two coordinated mechanisms:

1. A persisted lease row in `task_locks` identifies the owner and lease expiry.
2. A stable lock file at `.todoing/locks/<task-id>.lock` provides a short-lived OS access barrier.

Ordinary task operations acquire a shared OS lock before checking the database lease and accessing task data. The shared guard remains held until the command result has been serialized and emitted.

Lock acquisition, release, and stale-lease recovery acquire the exclusive OS lock. Exclusive acquisition waits for existing shared operations to finish. Once the lease is committed and the exclusive guard is released, new non-owner operations acquire a shared guard, observe the lease, and are rejected before reading task data.

Owner authorization uses a random opaque token. The database stores only a cryptographic hash of the token. The token is returned once by lock acquisition and supplied to later CLI calls through `TODOING_LOCK_TOKEN`; it is not stored in task content or history. The owner ID is separate from the secret token.

Initial lease defaults:

- lease duration: 60 seconds;
- heartbeat interval: 15 seconds;
- an expired lease may be recovered only while holding the exclusive OS barrier.

The owner may read and mutate the locked task while presenting the valid token. Other clients receive `TASK_LOCKED` for explicit task operations. List/filter operations omit tasks locked by another owner and do not expose their contents. A dependency on a locked task is reported as unavailable without exposing that task's content or state.

The lock file is a stable coordination object and is never deleted, renamed, or replaced. All Todoing clients must use the same guard protocol. The protocol protects Todoing clients, not arbitrary external programs that read the database directly.

The standard-library `File::lock_shared` and `File::lock` APIs are used. The repository MSRV is therefore Rust 1.89. The cross-platform prototype verifies the OS barrier primitive; lease authorization, renewal, expiry, and recovery still require dedicated integration tests.

## 17. CLI JSON contract

Every JSON response uses a versioned envelope.

Success:

```json
{
  "api_version": 1,
  "ok": true,
  "data": {}
}
```

Error:

```json
{
  "api_version": 1,
  "ok": false,
  "error": {
    "code": "TASK_NOT_FOUND",
    "message": "Task does not exist"
  }
}
```

Initial stable error-code groups:

- `INVALID_ARGUMENT`
- `TASK_NOT_FOUND`
- `STATUS_NOT_FOUND`
- `INVALID_TRANSITION`
- `DEPENDENCY_NOT_FOUND`
- `DEPENDENCY_CYCLE`
- `TASK_LOCKED`
- `LOCK_NOT_OWNER`
- `LOCK_EXPIRED`
- `VERSION_NOT_FOUND`
- `CONFLICT`
- `MIGRATION_ERROR`
- `STORAGE_ERROR`
- `INTERNAL_ERROR`

Initial process exit codes:

| Code | Meaning |
|---:|---|
| 0 | Success |
| 2 | CLI usage or argument parsing error |
| 3 | Validation or invalid transition |
| 4 | Requested resource not found |
| 5 | State/dependency conflict |
| 6 | Lock conflict or owner authorization failure |
| 7 | Storage, integrity, or migration failure |
| 8 | Unexpected internal failure |

The exact command names and per-command data schemas are specified with the CLI implementation. The JSON envelope and error-code meanings are versioned compatibility contracts.

## 18. Migration procedure

The migration command reads the current Markdown task files with YAML frontmatter. The JSON index is treated as rebuildable derived data; Markdown task files are the source of truth.

The importer must:

1. Read and validate all task files before declaring migration ready.
2. Preserve task IDs, titles, timestamps, status strings, labels, body text, Git commit references, and supported metadata.
3. Preserve unknown frontmatter fields in a legacy metadata payload or report them explicitly; never silently discard them.
4. Use a maintained YAML parser (`yaml_serde`) for legacy frontmatter.
5. Create a new temporary SQLite database and import records transactionally.
6. Restore the ID sequence above the maximum imported task ID.
7. Run SQLite integrity checks and application-level relationship checks.
8. Close/checkpoint the database and move it into the final store path only after successful validation.
9. Leave the original Markdown/YAML files and JSON index unchanged.
10. Refuse to overwrite an existing target database by default.

An invalid source record prevents migration from being reported as complete success. The exact report format and explicit recovery/override flags remain to be specified.

## 19. Remaining implementation risks

- The lease clock uses local UTC time; system clock jumps and suspend/resume behavior need explicit tests.
- A CLI list operation must hold the relevant shared guards until its response is emitted; otherwise a lock could be granted after data was read but before it was returned.
- A task lock does not protect direct external reads of the SQLite file. The public contract assumes access through Todoing.
- Backup/restore is post-MVP, but the SQLite online backup API must be used rather than copying a live WAL database file.
- The initial configuration and JSON envelope require schema/contract tests before implementation.
