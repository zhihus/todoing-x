# Target Architecture: Todoing Core

**Status:** Architecture overview. Resolved technical choices are specified in implementation-contract.md.
**Date:** 2026-11-21
**Scope:** Core MVP defined by PRODUCT.md and OpenSpec change core-task-capabilities.
**Normative reference:** implementation-contract.md records the selected storage, configuration, locking, and API baseline.

## 1. Architectural goals

Todoing is a standalone, local-first Rust CLI for managing work items. It is not an agent harness, orchestration engine, web service, or Manifest module.

The target architecture must provide:

- Reliable transactional task state, immutable versions, and append-only operation history.
- Configurable project workflow statuses and transitions.
- Task dependencies with cycle prevention.
- Exclusive work-session locks, including read exclusion for non-owners.
- A stable, versioned JSON interface for automation.
- Safe migration from the existing Markdown/YAML task store.
- Windows and Unix-like support without a database server.
- A small, maintainable implementation surface.

## 2. Proposed architecture style

Use a **modular monolith in one Rust crate**. Keep the executable synchronous and command-driven. Do not introduce a daemon, network server, async runtime, plugin framework, or workspace split for the MVP.

Separate responsibilities into four layers:

1. **Presentation** — Clap command parsing, human output, JSON serialization, process exit codes.
2. **Application** — use cases, validation orchestration, transaction boundaries, actor/lock context.
3. **Domain** — task, status, outcome, dependency, version, history, and lock rules; no database or CLI dependencies.
4. **Infrastructure** — storage adapter, configuration loader, legacy importer, backup/snapshot adapter, Git reference resolver.

The application layer depends on domain types and narrow ports. Infrastructure implements those ports. Domain logic must remain testable without opening a database or invoking a process.

### Proposed source layout

```text
src/
  main.rs
  lib.rs
  domain/
    task.rs
    status.rs
    dependency.rs
    version.rs
    history.rs
    lock.rs
    error.rs
  application/
    commands/
    queries/
    transaction.rs
    context.rs
  ports/
    task_store.rs
    config_store.rs
    clock.rs
    git.rs
  infrastructure/
    sqlite/
      connection.rs
      schema.rs
      migrations.rs
      task_store.rs
      lock_store.rs
      history_store.rs
    config/
      loader.rs
    migration/
      markdown_import.rs
    backup/
      snapshot.rs
  cli/
    commands.rs
    human_output.rs
    json_output.rs
```

This is a logical layout; files can be grouped more compactly while the implementation remains small. Avoid premature crate extraction.

## 3. Storage architecture

### 3.1 Preferred candidate

Use one embedded SQLite database per project, accessed through rusqlite with the bundled feature. This is the selected MVP storage baseline.

The cross-platform proof of concept passed on ubuntu-latest and windows-latest. It verified the rollback-journal lock-acquisition barrier, non-owner read rejection, concurrent ID allocation, rollback after abrupt process termination, and fencing-token rejection.

**Selected MVP settings:**
- Rollback-journal mode (DELETE), not WAL.
- FULL synchronous mode until durability tests justify a change.
- Foreign-key enforcement enabled and verified on every connection.
- A 10-second busy timeout and explicit storage-busy error mapping.
- A short EXCLUSIVE transaction for task-lock acquisition.

The journal-mode recommendation is tied to the lock requirement: the tested EXCLUSIVE transaction waits for existing readers in rollback-journal mode. WAL has different reader behavior and does not provide that same barrier. Do not switch to WAL without a replacement mechanism and equivalent cross-platform tests.

SQLite is selected. redb 5.x remains a contingency alternative only if production verification exposes a requirement SQLite cannot satisfy.

### 3.2 Project store layout

Proposed layout:

```text
.todoing/
  todoing.db
  config.toml
  migration/
    reports/
```

The legacy .todoing/tasks/ Markdown files and index.json remain untouched during migration and are retained as read-only recovery material after a successful cutover. They are not a second writable source of truth.

A store metadata record contains the schema version and a stable project identifier. Human CLI invocations resolve the store from the project root. Manifest-facing JSON API requests supply the expected project identifier, which Todoing verifies against store metadata.

### 3.3 Logical relational entities

The following database entities are required by the selected SQLite architecture. This is a logical model, not the final SQL schema.

| Entity | Responsibility |
|---|---|
| store_metadata | Schema version, project ID, store metadata. |
| tasks | Stable task ID and current task fields. |
| task_dependencies | Directed dependency edges with foreign keys and uniqueness. |
| task_versions | Immutable snapshots for each successful task save. |
| task_baselines | The user-approved baseline version for each task. |
| operation_history | Append-only successful and rejected operation records. |
| task_locks | Current owner, token hash, lease, heartbeat, and fencing generation. |
| task_labels | Ordered labels with normalized lookup keys. |
| task_commit_refs | Ordered full Git commit SHAs. |

Preserve the existing task fields: numeric ID, title, creation timestamp, status, labels, body, and Git commit references. Existing numeric IDs should be imported without change. Priorities are not part of the model.

Use database constraints for unique IDs, foreign keys, unique dependency edges, and valid references. Dependency-cycle validation must run in the same transaction as the relationship update so concurrent opposing changes cannot both pass against stale graph state.

## 4. Task mutation and history transaction

Every successful task save creates a new immutable version. The task's current row, new version snapshot, successful operation-history entry, and related dependency/metadata changes commit atomically.

### Successful mutation flow

1. Parse the request and establish project, actor, and lock context.
2. Load one validated status-configuration snapshot for this CLI invocation.
3. Begin the appropriate database transaction.
4. Check task existence, active lock ownership, fencing generation, status transition, and domain constraints.
5. Validate dependency graph changes.
6. Apply the current task state.
7. Append the immutable version snapshot.
8. Append the successful operation-history entry.
9. Commit once; only then return success to the caller.

Any failure before commit rolls back the task, version, history, and related changes together.

### Rejected operation history

Rejected task-operation attempts that reach the application operation layer must also be recorded with actor, timestamp, attempted operation, relevant previous/requested values, and rejection outcome. A rejected operation does not create a task version or change committed task state.

Because the rejected operation itself must be logged, the implementation should roll back the attempted mutation and then append the rejection event in a separate transaction. If the database is unavailable or cannot accept writes, the CLI must report that the rejection could not be recorded; it must not claim that the history entry exists. This limitation should be made explicit in error handling and tests.

### Version and baseline behavior

- A task creation creates its initial version.
- Each successful save creates a new version; rejected operations do not.
- Versions are immutable and individually inspectable.
- The user-approved baseline points to an existing version and is changed only by an explicit approval operation.
- Comparing versions is read-only and does not mutate either version.
- The exact snapshot serialization and diff algorithm remain implementation details, but snapshots must preserve all user-visible task fields and compare deterministically.

## 5. Task-lock protocol

Database transaction locks and logical task locks are different mechanisms. SQLite protects database consistency; Todoing's lock protocol represents an agent's exclusive ownership of one task across multiple CLI invocations.

### 5.1 Lock record

A logical lock contains:

- Task ID.
- Owner/session ID.
- Hash of a secret ownership token; never persist or log the raw token.
- Acquisition time, last heartbeat, and lease expiry.
- Monotonically increasing fencing generation.
- Recovery/release metadata.

The ownership token proves possession; the actor/owner ID provides attribution. Local Todoing is not an authentication service, so actor identity is attribution rather than a security identity.

### 5.2 Acquisition

For the SQLite rollback-journal candidate:

1. Start a short EXCLUSIVE transaction.
2. Wait for already-running read transactions to finish.
3. Verify that the task exists and no unexpired lock is held.
4. Assign the owner, token hash, lease, and next fencing generation.
5. Append the lock-acquired operation event.
6. Commit; this commit is the lock-acquisition linearization point.

The EXCLUSIVE transaction briefly blocks database readers globally, not just readers of the selected task. It is held only for lock acquisition. The process-level prototype demonstrated the barrier on Windows and Linux. The selected defaults are a 300-second lease, 60-second heartbeat, and 60-second recovery grace.

### 5.3 Reads and mutations

Every task read must check the lock record and ownership in the same read transaction as the task query. Every mutation must check ownership and fencing generation in the same write transaction as the mutation. List/search operations omit tasks locked by another client; direct reads return a lock conflict without task content.

- No active lock: normal client access is permitted.
- Active lock owned by this client: access is permitted with the valid token.
- Active lock owned by another client: reads and writes are rejected with a stable lock-conflict error.
- Expired lock: non-owner access remains rejected until explicit recovery completes; expiry must not silently grant ownership.
- Stale owner: any command with an old fencing generation is rejected, even if the old process resumes.

All storage access must go through the lock-aware application/store layer. Direct file access to task data is not supported.

### 5.4 Heartbeat, release, and recovery

- Heartbeat renews a lease only if owner ID and fencing generation still match.
- Explicit release is accepted only from the current owner.
- Recovery is a transactional compare-and-swap on an expired lease.
- Successful recovery increments the fencing generation.
- A stale client cannot renew, read, or mutate after recovery.

Lease duration, heartbeat cadence, clock-change policy, and exact recovery thresholds remain open. The implementation must test process termination, heartbeat/recovery races, clock jumps, sleep/resume, and two simultaneous recovery clients.

## 6. Configurable workflow

### 6.1 Configuration source

TOML is selected for the user-edited project status configuration. JSON is used for the machine interface. YAML is retained only as a legacy migration format.

Configuration location: .todoing/config.toml. Format version 1 and the default lock settings are defined in implementation-contract.md.

The loader parses and validates the complete configuration once per CLI invocation. All operations in that invocation use the same immutable configuration snapshot. The configuration revision/fingerprint should be included in relevant history metadata so that a transition can be traced to the rules that permitted it.

### 6.2 Status definition

Each status has:
- Stable machine identifier.
- Human-readable label.
- Enabled/disabled flag.
- Optional terminal outcome mapping.
- Optional presentation metadata, only if a real CLI need is established.

Allowed transitions are declared separately. No universal Todoing workflow or Manifest-specific status names are hard-coded.

A disabled status remains readable in historical/current records but cannot be selected for a new transition.

### 6.3 Completion and cancellation

Workflow status and task outcome are separate concepts.

Selected model:
- Status represents the project's current workflow position.
- Outcome is absent for active tasks and explicitly set to completed or cancelled for terminal results.
- A configured status may map to completed (for example, Done).
- The explicit cancellation operation sets cancelled without changing workflow status and accepts an optional reason.
- Reopen clears the outcome; any status change then follows normal configured transition rules.

This preserves the distinction between a configurable status label and the semantic result.

## 7. CLI and JSON interface

### 7.1 CLI role

The CLI remains the primary interface for humans and agents. Preserve existing command names where their semantics remain valid, and add commands for dependencies, history, versions, locks, configuration validation, migration, and store verification.

Existing behavior requiring a compatibility change:
- Physical delete becomes a rejected operation; tasks are never physically deleted.
- Direct editing of Markdown files cannot remain the storage mechanism. An editor workflow may use a temporary representation and save through the normal transactional path.
- Reindexing Markdown files is replaced by database integrity/verification commands after migration.
- Manifest-specific bundled completion hooks are not part of the target core. Manifest owns OpenSpec/Git workflow policy. Generic optional hooks/events remain post-MVP.

The exact command tree and aliases are to be specified before implementation.

### 7.2 JSON contract

All machine-readable responses must identify the API version, return deterministic JSON, and keep diagnostics on stderr.

Version 1 envelope, defined normatively in implementation-contract.md:

```json
{
  "api_version": "1",
  "ok": true,
  "data": {}
}
```

Version 1 error envelope, defined normatively in implementation-contract.md:

```json
{
  "api_version": "1",
  "ok": false,
  "error": {
    "code": "task_locked",
    "message": "Task is locked by another client",
    "details": {}
  }
}
```

The API v1 request/response envelope is defined in implementation-contract.md and docs/api/v1/request.schema.json / docs/api/v1/response.schema.json. Per-operation argument and response schemas and their automated validation remain implementation work.

## 8. Migration from the current store

The current Markdown task files are the source records; index.json is a rebuildable index. Migration must not treat the index as more authoritative than the Markdown task files.

Proposed migration flow:

1. Detect the existing .todoing/tasks/*.md files and index.json.
2. Read and validate every source task; report malformed records, duplicate IDs, invalid statuses, and unsupported metadata.
3. Generate or validate a project status configuration that preserves existing status values. Do not silently map unknown statuses.
4. Build a new database in a temporary/staging location.
5. Import tasks with original IDs, timestamps, titles, statuses, labels, bodies, and Git references.
6. Create an initial version and a clearly labelled migration-import history event for each task. Do not invent historical edits that the legacy store never recorded.
7. Verify task counts, identifiers, field values, commit references, and database integrity against the source.
8. Activate the new database only after verification.
9. Keep the original Markdown files and index unchanged as read-only recovery material.

No dual-write period is permitted: it would create two competing sources of truth. If migration fails, the original store remains usable and the staged database can be discarded or inspected.

The exact activation/rename procedure must account for Windows file replacement semantics and open file handles. Migration must be repeatable or safely detect that it has already completed.

## 9. Backup and recovery boundary

Manifest/the consuming project owns backup destination, retention, and off-device replication. Todoing exposes a database-aware snapshot operation using SQLite's Online Backup API through rusqlite. Do not copy a live database file blindly. A produced backup must be reopened and integrity-checked before being reported as valid.

Git history is not a backup mechanism. Archive/restore and full task import/export remain post-MVP capabilities.

## 10. Error model

The application layer should distinguish at least:

- Invalid input/configuration.
- Task not found.
- Invalid status transition.
- Dependency cycle.
- Active task lock / stale fencing token.
- Duplicate or conflicting operation.
- Storage busy/timeout.
- Corrupt or unsupported store schema.
- Migration validation/conflict failure.
- Persistence failure.
- Operation rejected but rejection-history append failed.

Each error maps to a stable machine code and documented process exit code. Human diagnostics may be more descriptive, but must not change the machine contract.

## 11. Testing strategy

Required test layers:

1. **Domain unit tests:** status rules, outcomes, validation, dependency cycle detection, version/baseline rules.
2. **SQLite repository tests:** constraints, foreign keys, atomic task/version/history commit, rollback, schema upgrades.
3. **Multi-process tests:** simultaneous ID allocation, lock acquisition barrier, read exclusion, heartbeat/recovery races, stale fencing.
4. **Failure injection:** process termination during an uncommitted transaction, failure between logical mutation steps, disk-full/IO errors where practical.
5. **Migration fixtures:** legacy Markdown/YAML variants, missing/corrupt index, duplicate IDs, invalid metadata, partial migration, recovery.
6. **API contract tests:** JSON schema snapshots, stable error codes, stdout/stderr separation, exit codes.
7. **Cross-platform CI:** Windows and Linux at minimum for every storage, locking, migration, and CLI release change.

The existing SQLite spike is a reference test, not a substitute for production repository tests.

## 12. Explicitly outside the MVP

- Archive/restore implementation.
- Full task import/export beyond legacy migration.
- Generic lifecycle event delivery and extensible hook framework.
- Backup scheduling/retention/remote replication.
- UI or web service.
- Manifest/OpenSpec/Git orchestration rules inside Todoing.
- Priority fields or a priority-driven scheduler.

## 13. Remaining implementation-level questions

The implementation contract resolves the core technology and behavioral choices. These details must be verified while implementing, without changing the accepted contract silently:

1. Final SQL DDL, indexes, triggers, and migration numbering.
2. Per-operation JSON request/response schemas and generated validation.
3. Clock-jump and machine sleep/resume handling for leases.
4. Windows same-volume database activation and recovery behavior.
5. Online Backup API behavior while writes continue.
6. Exact CLI aliases and deprecation period for legacy commands.
7. Rejection-history behavior when the database itself is unavailable.
8. Failure-injection coverage for disk-full, interrupted commit, and recovery races.

## 14. Implementation sequence

1. Freeze domain types and error taxonomy.
2. Add configuration parsing and validation with fixture tests.
3. Add SQLite schema/migrations and transaction abstraction.
4. Implement task CRUD without physical deletion, with version/history transactions.
5. Implement statuses, outcomes, and dependency validation.
6. Implement lock lease, acquisition barrier, heartbeat, release, recovery, and fencing.
7. Implement version inspection, comparison, and baseline approval.
8. Implement stable JSON responses and exit codes.
9. Implement legacy migration and verification.
10. Complete cross-platform failure/concurrency tests and update current-behavior OpenSpec specs only after behavior is implemented and verified.
