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

**Recommendation:** TOML in the project-local Todoing directory, based on the configuration-format evaluation. This is not yet an approved schema or implementation decision. **Open:** exact path, field schema, schema versioning, defaults for a new project, and behavior when a referenced status definition is missing. See `docs/architecture/research/status-config-format.md`.

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

The leading candidate for a prototype is a hybrid protocol: persist the long-lived owner lease in the transactional store, and use short-lived per-task shared/exclusive OS file locks as an access barrier. The exclusive barrier drains in-flight operations before lock acquisition; later operations check the persisted lease while holding a shared barrier. This avoids holding a database transaction for the full agent session.

A dedicated local coordinator remains an alternative if the hybrid protocol cannot provide reliable cross-platform behavior. The hybrid protocol is not approved until Windows and Unix multi-process tests prove strict read exclusion, in-flight operation handling, and crash recovery. See `docs/architecture/research/task-lock-protocol.md`.

A task lock must not be implemented by holding a long-lived database transaction for the entire agent work session.

## 8. Storage engine boundary

The application layer depends on a storage interface that can express atomic units of work, uniqueness constraints, and concurrent access results.

SQLite via rusqlite is the leading prototype candidate. redb is the required comparison candidate. The storage engine remains undecided until the cross-platform, multi-process, crash-recovery, lock, migration, and backup tests in the research note are completed.

No database-specific schema, SQL dialect, or file layout is approved by this draft.

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

1. Final storage engine after prototype and failure testing.
2. Task lock protocol, including strict read exclusion and in-flight read handling.
3. Status configuration format, schema, and default configuration.
4. Completion/cancellation outcome representation and workflow mapping.
5. Actor identity model.
6. Version snapshot and comparison semantics.
7. JSON contract versioning, envelope, error codes, and exit-code mapping.
8. Migration conflict, rerun, and unknown-field policy.
9. Backup/restore integration contract and integrity checks.

Implementation must not silently resolve these open decisions.
