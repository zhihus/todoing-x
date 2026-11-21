# Target Architecture — Todoing MVP

**Status:** Proposed logical architecture; storage engine and several protocols remain undecided  
**Date:** 21 November 2026  
**Scope:** Standalone Todoing core and its public CLI/JSON interface  
**Authority:** Companion elaboration to `openspec/changes/core-task-capabilities/design.md`. The OpenSpec proposal/specs/design remain normative; this document expands the logical architecture and must not override them.

## 1. Architectural goals

The target implementation must satisfy the accepted product baseline while remaining independent of Manifest, OpenSpec, Git workflow, and agent harnesses.

The architecture is organized around five invariants:

1. A task store belongs to one project; unrelated projects do not share an MVP database.
2. A successful task mutation, its version, its operation-history record, and all required state updates commit atomically.
3. A task lock is exclusive for the full work session and blocks reads as well as writes by other clients.
4. Task identifiers are stable, never reused, and tasks are never physically deleted.
5. The CLI and versioned JSON contract are adapters over the same application operations; they do not implement separate business rules.

## 2. Logical components

The implementation should use explicit boundaries rather than putting all behavior in CLI command handlers.

| Component | Responsibility |
|---|---|
| CLI adapter | Parse commands and arguments; format human output; preserve predictable exit behavior |
| JSON contract adapter | Validate request/response versions; serialize deterministic success and error envelopes |
| Application services | Execute use cases and enforce transaction boundaries, authorization, and validation |
| Domain model | Task, configured status, outcome, dependency, version, history entry, and lock concepts |
| Storage port | Transactional repositories and unit-of-work boundary; hides the selected engine |
| Storage adapter | Implements transactions, constraints, queries, migrations, and consistent snapshots for the selected engine |
| Configuration | Loads and validates project configuration without embedding Manifest policy |
| Migration service | Imports the current Markdown/YAML and JSON store and verifies migrated content |

Suggested Rust module boundaries:

- `domain`: types, invariants, status and dependency rules
- `application`: task, dependency, history, version, and lock use cases
- `storage`: repository interfaces, transaction boundary, schema migrations, engine adapter
- `api`: versioned JSON DTOs, error codes, serialization
- `cli`: clap definitions and human-readable presentation
- `migration`: legacy-store inspection, import, validation, and cutover
- `config`: project configuration loading and validation

These are logical boundaries, not a final crate layout.

## 3. Conceptual data model

The model is independent of the final database schema.

| Concept | Purpose | Key invariant |
|---|---|---|
| Project metadata | Store identity and schema/configuration version information | Every store is bound to one project |
| Task | Current task fields and current workflow status/outcome | Stable ID; no physical deletion |
| Status definition | User-defined status value, display order, enabled state, and allowed transitions | No built-in Manifest workflow |
| Dependency | Directed relationship between prerequisite and dependent task | No self-edge or cycle |
| Task version | Immutable snapshot of task data at a successful save | Every successful save creates one version |
| Operation history | Append-only record of actor, time, operation, previous/new value, and outcome | Existing entries are never edited |
| Approval marker | Identifies the user-approved baseline version | Later saves do not silently move the baseline |
| Task lock | Owner identity, lease/expiry, and renewal state | One owner at a time; non-owners cannot read or mutate the task |
| ID sequence | Allocates task identifiers | Allocation is atomic; IDs are never reused |

The exact representation of completion and cancellation alongside configurable statuses is unresolved. The model must preserve the semantic distinction without assuming that a particular status string is the outcome itself.

Dependency records must be queryable in both directions, and clients must be able to inspect each prerequisite's current state. Whether a particular dependency state blocks a status transition is a separate product decision and is not defined by this change.

History and version records must remain separate concepts: history records operations and outcomes; versions preserve task snapshots. A successful save creates a version, but not every history event necessarily changes task content.

## 4. Transaction boundaries

Every operation that changes task state must execute through one application-level unit of work.

### Create task

Within one transaction:

1. Validate the configured initial status.
2. Allocate a unique, never-reused task ID.
3. Insert the task.
4. Create its initial version.
5. Append the corresponding operation-history entry.
6. Update any required derived state.
7. Commit.

If any required step fails, no partial task creation becomes visible.

### Update task

Within one transaction:

1. Resolve the task and validate the caller's lock ownership.
2. Validate field changes, status transitions, and dependency invariants.
3. Apply the task mutation.
4. Create a new immutable version.
5. Append the operation-history entry.
6. Update required derived state.
7. Commit.

Rejected task-operation attempts that reach the task operation layer are recorded with their outcome. They do not create a successful task version or mutate committed task state. A storage failure that prevents writing the history entry cannot itself be recorded; the required error and recovery behavior remains to be specified.

### Dependency mutation

Within one transaction:

- Validate both task references.
- Reject self-dependencies and cycles.
- Write the relationship.
- Append the operation history.
- Commit the complete graph change.

### Lock operations

Lock acquisition, renewal, and release are short atomic operations. They must not hold a database transaction open for the duration of agent work.

## 5. Locking and authorization model

A long-lived task lock must be represented as durable lease state, not as a database/file lock held by an open process. The technical research identifies a stricter acquisition barrier as a key proof obligation: existing operations must finish before acquisition succeeds, and subsequent non-owner reads and writes must be rejected.

A database transaction lock is not the same as the task-work lock. A short database-wide barrier may be needed while acquiring the durable task lease, but must not be held for the duration of agent work. With SQLite, rollback-journal `BEGIN EXCLUSIVE` is one candidate barrier; in WAL mode, `EXCLUSIVE` behaves like `IMMEDIATE` and does not exclude readers. The journal mode therefore remains open until a multi-process acquisition-race prototype passes on supported platforms.

Conceptual flow:

1. A client requests a lock for a task.
2. The application atomically checks whether a live lock exists.
3. If no live lock exists, it records the owner and lease and returns the information needed to prove ownership.
4. The owner renews the lease while work continues.
5. Every task operation, including reads, verifies the lock owner inside the same short transaction used for the operation.
6. Explicit release removes or closes the active lease.
7. An expired lease can be recovered under the defined recovery protocol.

The owner credential must not be guessable or interchangeable with a display name. Its representation and transport are unresolved.

A non-owner must not bypass the lock by using a read-only command, JSON mode, index/list command, or alternate CLI path. The exact scope of list operations and how they behave when one listed task is locked must be specified before implementation.

The design must define operation ordering for a read that begins concurrently with lock acquisition. Existing reads must complete before acquisition reports success; after success, non-owner reads must be rejected. The authorization check and data read must have a clear linearization point. A monotonically increasing fencing generation must prevent a stale owner from mutating the task after lease expiry and recovery.

## 6. Concurrency model

Todoing is a multi-process CLI even if each project is normally used by one agent at a time. Separate invocations must safely share one project store.

- ID allocation must be part of a serialized atomic operation.
- Task mutation must use optimistic version checks or equivalent transaction isolation so stale clients cannot overwrite newer state silently.
- Lock acquisition must be atomic.
- Heartbeat and expiry must tolerate delayed or duplicated requests.
- All retries must be bounded and return stable conflict/busy errors when they cannot proceed.
- No application operation may bypass the storage and authorization layer.

SQLite and redb both serialize writes in their documented transaction models. This is acceptable as a starting point, but lock contention and throughput must be measured under representative multi-process tests.

## 7. Storage engine boundary

The application layer must depend on a storage transaction/repository abstraction, not directly on SQLite or redb types.

Current research recommends SQLite through `rusqlite` with the bundled feature as the first prototype candidate. redb 5.x is the strongest alternative; LMDB/heed is secondary. RocksDB, fjall, and sled have also been assessed. See [Technical Research: Storage, Concurrency, and Configuration](technical-research.md).

The SQLite journal mode is deliberately unresolved until the task-lock acquisition barrier has been proven. The preliminary configuration-format recommendation is TOML for user-edited status configuration and JSON for the machine interface; neither format's final schema is approved.

The storage adapter is responsible for:

- Atomic transactions across task, version, history, dependency, lock, and sequence data.
- Constraint enforcement and schema/index maintenance.
- Schema versioning and database migrations.
- Consistent backup/snapshot creation.
- Integrity checks and recovery diagnostics.

The merged OpenSpec design also requires rejected task-operation attempts that reach the task-operation layer to be recorded without creating a task version.

No final table schema, database filename, journal mode, connection pool, or Rust storage crate dependency is approved by this document.

## 8. CLI and JSON contract

Detailed options for actor identity, operation-history semantics, version snapshots/comparison, JSON envelopes, error codes, and migration are recorded in [Design Proposals: History, JSON Contract, and Migration](api-history-migration-design.md). Those recommendations remain proposals until accepted in the decision register.

The CLI is the primary user and agent interface. Command handlers must call application services and must not contain independent business logic.

The JSON interface must be explicitly versioned and deterministic. The contract must define:

- Request and response version negotiation.
- Stable operation names and input schemas.
- Stable success response envelopes.
- Stable error codes, error details, and process exit-code mapping.
- Standard-output and diagnostic-stream rules.
- Compatibility guarantees within a major contract version.

The command surface must cover task operations, status configuration, dependencies, history/version inspection and comparison, and lock operations.

The exact command names, flags, JSON envelope, and version negotiation mechanism remain open. The current `--json` option is a compatibility starting point, not the target contract.

## 9. Configuration

Status definitions are user-editable project configuration. Configuration must be validated before it is used for task operations.

The design must settle:

- Final file format and schema (TOML is the preferred candidate for user-edited configuration; JSON remains the machine-interface format candidate).
- Default configuration for a newly initialized project.
- Status ordering and display labels.
- Permitted transition representation.
- Behavior when a status is disabled while existing tasks still use it.
- How completion and cancellation outcomes relate to workflow statuses.
- How concurrent configuration edits are detected and applied.

Todoing must not ship Manifest's six workflow statuses as mandatory built-in behavior.

## 10. Migration

Migration from the existing format is a required MVP capability. Detailed source-conflict, preservation, cutover, and rollback options are recorded in [Design Proposals: History, JSON Contract, and Migration](api-history-migration-design.md).

The migration service must:

1. Inspect the legacy `.todoing/tasks/*.md` files and `.todoing/index.json`.
2. Identify malformed, duplicate, conflicting, or unsupported source records.
3. Preserve task IDs, titles, timestamps, statuses, labels, bodies, and Git commit references wherever representable.
4. Report data that cannot be represented rather than silently dropping it.
5. Import into the target store.
6. Validate record counts, identifiers, relationships, and field values.
7. Leave the source store unchanged until the user has a verified migrated store.

The exact cutover, repeatability, rollback, and handling of divergent Markdown/index data require a migration design. Git history is not a substitute for migration validation or backup.

## 11. Backup and recovery

Backup remains the responsibility of the consuming project/framework, but Todoing must expose a safe snapshot mechanism.

- A backup must represent one consistent committed database state.
- A live database must not be copied using an unsafe raw-file copy procedure.
- Restore must be followed by integrity validation.
- Backup targets must support a separate physical disk or remote storage.
- Database recovery and backup recovery must be tested independently.
- Git may version project configuration or exported data, but is not the transaction manager or backup system.

## 12. Testing strategy

TEST FIRST applies to all implementation work.

Required test layers:

- Domain tests: transitions, disabled statuses, completion/cancellation distinction, dependency cycles.
- Storage tests: atomic rollback, unique ID allocation, version/history consistency, referential integrity.
- Lock tests: exclusivity, read exclusion, lease renewal, expiry, crash recovery, and acquisition races.
- CLI/API tests: contract versioning, deterministic JSON, stable errors, and exit codes.
- Multi-process tests: independent CLI processes against the same project store.
- Failure injection: terminate or fail operations between logical write stages and around commit boundaries.
- Migration tests: legacy fixtures, malformed data, preservation checks, and failure rollback.
- Platform tests: Windows and Linux at minimum; macOS-specific behavior if macOS is in the supported platform matrix.
- Backup tests: concurrent snapshot, restore, and integrity verification.

Performance tests must use representative task counts and history/version growth. Acceptance thresholds must be defined before interpreting benchmark results.

## 13. Decisions still required

1. Final storage engine after SQLite/redb prototype comparison, including journal mode and the task-lock acquisition barrier.
2. Physical schema and schema migration/versioning strategy.
3. Status configuration syntax and completion/cancellation representation.
4. Whether dependency state blocks status transitions and how prerequisite states affect that policy.
5. Lock owner identity, credential transport, lease duration, heartbeat, fencing, and recovery rules.
6. Lock-aware list behavior and read/acquisition linearization.
7. Which events outside the task-operation layer belong in history, and how failures that prevent a history write are surfaced.
8. Version snapshot format, diff semantics, and approval operation.
9. JSON contract envelope, version negotiation, error catalogue, and CLI command names.
10. Legacy migration conflict resolution and cutover/rollback behavior.
11. Supported filesystem/location policy, especially network-mounted project directories. SQLite WAL is not suitable for network filesystems.
12. Backup snapshot mechanism and verification contract.
