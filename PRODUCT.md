# Todoing — Product Definition

**Status:** Product baseline for review  
**Scope:** Standalone Todoing product; Manifest integration is a separate consumer-specific layer.

## 1. Purpose

Todoing is a local-first, project-scoped task manager for humans and AI agents. It provides a reliable task store and a machine-friendly CLI so agents can create, inspect, update, and transition tasks without depending on a particular agent harness or orchestration framework.

Todoing is an independent Rust application. It must remain useful without Manifest.

## 2. Product boundaries

### Todoing owns

- Task records and their current state.
- User-configurable statuses and permitted status transitions.
- Task dependencies.
- Task versions and an append-only operation history.
- Exclusive task locks while an agent is working on a task.
- Atomic persistence of task changes and their associated history/state.
- A CLI and a stable, versioned JSON interface.
- Generic, optional lifecycle events/hooks.
- Task archival and restoration as separate operations.
- Import/export and data-integrity validation (post-MVP).

### Todoing does not own

- Agent orchestration, role assignment, model selection, or harness configuration.
- OpenSpec specifications or OpenSpec workflow.
- Git branch, commit, review, merge, release, or project-governance policy.
- Manifest-specific statuses, gates, hooks, or validation rules.
- A central multi-project service or mandatory graphical interface.

Manifest may consume Todoing through its public CLI/JSON contract and apply its own policies externally. Todoing must not contain Manifest-specific logic or require Manifest to operate.

## 3. Operating model

- Each project has its own Todoing task store.
- An agent operates on the task store belonging to its current project.
- A central task database shared by unrelated projects is out of scope for the MVP.
- Git versions only Todoing application source, specifications, tests, and documentation. Project task data is private local data and MUST NOT enter any Git repository.
- Todoing must preserve data integrity independently of whether the project is under Git.
- Each project has its own private `.todoing/` directory, excluded from Git. It contains the database, user configuration, legacy source data, migration reports, and snapshots. Human CLI initialization uses `todoing init` and generates a project ID when `--project-id` is omitted. JSON API initialization uses store.init with an explicit project_id; every later API request must provide that same ID, which Todoing verifies against database metadata. The consuming framework owns the stable ID. A project ID is a namespace guard, not an authentication credential.

## 4. Task lifecycle and status configuration

- Statuses are configurable by the user; Todoing must not hard-code a single workflow.
- The status configuration is stored in the user-editable .todoing/config.toml file, using TOML format version 1.
- Users control the available statuses and their ordering. Permitted transitions must be representable and validated.
- Disabling a status prevents its use for new transitions but must not erase historical records that used it.
- Manifest may publish a recommended status configuration. Todoing remains workflow-agnostic and does not automatically synchronize or impose Manifest statuses.
- The Manifest workflow currently uses: Backlog, Ready, In Progress, In Review, Reviewed, and Done (display labels may be localized by the project).
- Completion and cancellation are distinct nullable task outcomes, independent of workflow status. A configured status may map to an outcome; explicit complete/cancel operations can set an outcome without changing status. Reopening clears the outcome under the configured transition rules.

## 5. Tasks, dependencies, and deletion

- Tasks have stable identifiers; identifiers must never be reused.
- Tasks support structured metadata and content sufficient for both human and agent workflows.
- Tasks can depend on other tasks. The system must detect dependency cycles and expose dependency state to clients.
- Physical deletion of tasks is not supported.
- Archiving is a separate operation, not a status and not an automatic consequence of completion or cancellation.
- Archived tasks are removed from the active board/list but remain available in an archive view and retain their history and relationships.
- Restoration from the archive is required.
- Agents may read archived tasks when they are not locked.
- Automatic archival policies (age/active-task limits) are not part of the MVP and require explicit configuration and specification.

## 6. History and versions

- Every successful task save creates a new task version.
- The system maintains a complete, append-only operation history: actor, timestamp, operation, previous value, new value, and outcome.
- Existing history entries are immutable. Corrections are recorded as new operations.
- The version approved by the user is identifiable as an approval/baseline version.
- Clients can inspect and compare task versions.
- Task mutation, version creation, history append, and state/index updates must be atomic as one logical operation.

## 7. Locking and concurrency

- A task can be exclusively locked for the full duration of an agent's work on it.
- While a task is locked, other clients cannot perform task operations, including reads.
- The lock protocol includes acquisition, owner identity, heartbeat/lease renewal, release, and recovery after an abnormal process termination.
- A configurable or well-defined timeout prevents abandoned locks from blocking work forever.
- Concurrent operations must not allocate duplicate task IDs or overwrite one another silently.
- Manifest's current workflow runs agents sequentially. Todoing must nevertheless protect its data against concurrent clients because it is an independent tool.

## 8. CLI and machine interface

- The CLI is the primary interface for humans and agents.
- Machine-readable JSON output is a supported contract, not an incidental rendering option.
- The JSON contract must be versioned, documented, deterministic, and backward-compatible within a declared version.
- Errors must have stable machine-readable codes and predictable process exit codes; diagnostics must not corrupt JSON output on stdout.
- Commands must expose task operations, status configuration, dependencies, history/version inspection, and lock operations.
- The CLI and JSON API v1 operation set is documented in README.md and docs/api/v1/operations.md. API v1 processes one JSON request per todoing api invocation, reading stdin and writing one response to stdout. The store.init operation initializes a new store before ordinary store operations.

## 9. Events and hooks

- A generic lifecycle event/hook mechanism is a deferred capability; it is not part of the current SQLite backend contract.
- Todoing does not know what OpenSpec, Manifest, or a particular harness is and does not enforce their policies.
- Consumers may integrate optional checks externally, but core correctness must not depend on handlers.
- Event payload schema, delivery guarantees, ordering, failure handling, and configuration remain undecided until a concrete use case justifies the design.

## 10. Git references, backup, and portability

- Tasks may store references to Git commits as traceability metadata.
- Todoing does not enforce project Git workflow or treat a commit as proof that a task is complete.
- Git history is not a substitute for task history, transactional persistence, or backup.
- Todoing provides a verified SQLite Online Backup API snapshot. The consuming project/framework owns destination selection, retention, and replication.
- Full import/export must preserve tasks, versions, history, dependencies, links, and metadata, and validate integrity; it remains post-MVP.
- A non-destructive migration from legacy Markdown/YAML task files and index.json to SQLite is implemented. Source files are retained unchanged as recovery material.

## 11. Storage

SQLite is the selected storage engine: one database per project, accessed from Rust through rusqlite with bundled SQLite. The implementation uses rollback-journal DELETE mode, synchronous=FULL, verified foreign keys, and a bounded busy timeout. The selection is based on the relational task model and cross-platform process-locking evidence documented in docs/architecture/implementation-contract.md and docs/architecture/technical-research.md.

The SQLite database is the sole writable source of task state after initialization or migration. Markdown/YAML remain legacy recovery material and may be used for import/export; they are not the transactional store.

## 12. Delivery scope

### MVP

1. Define the task model and user-configurable status model.
2. Implement reliable storage with atomic transactions.
3. Implement task operations and stable identifiers without physical deletion.
4. Implement dependencies and cycle detection.
5. Implement append-only history and task versions.
6. Implement exclusive locks, heartbeat, release, and abandoned-lock recovery.
7. Deliver the CLI and versioned JSON contract.
8. Provide migration planning and a safe migration path from the current format.

### After MVP

- Generic optional lifecycle events/hooks and consumer integration.
- Manifest integration contract, project identity binding, and cross-component consistency checks.
- Configurable archival policies; explicit archive/restore has been delivered.
- Import/export with full integrity validation.
- Backup automation and advanced diagnostics.
- A read-only multi-project UI, if later justified.

The core MVP capabilities are implemented: project-local SQLite storage and initialization, configurable statuses, task operations, dependencies, immutable history and versions, baselines, exclusive locks, CLI and JSON API v1, non-destructive legacy migration, archive/restore, and verified snapshots.

Deferred capabilities include generic lifecycle events/hooks, automatic archival policies, full import/export, backup automation, and consumer-side integration policy and consistency checks. The precise sequencing may be changed by explicit product decisions.

## 13. Compatibility and migration principles

- This Product Definition is normative. Implementation behavior must not silently override accepted product requirements.
- The Rust implementation is the active product implementation. The legacy Markdown/YAML task format remains a migration source and recovery format; it is not a second writable source of truth after SQLite initialization or migration.
- Breaking changes must be explicit, documented, and accompanied by migration tooling or a clear migration procedure.
- Migration must not silently lose task content, identifiers, commit references, or other recoverable metadata.
- No existing data may be physically deleted as part of routine migration.

## 14. Product invariants

1. Todoing remains independent of Manifest and agent harnesses.
2. The task store is the source of truth for task state, versions, and task history.
3. No physical task deletion and no identifier reuse.
4. Every successful mutation is atomic with its history/version effects.
5. A task lock is exclusive for the full work session.
6. Status configuration is user-controlled; no hard-coded Manifest workflow.
7. Machine interfaces are stable, versioned contracts.
8. Git contains application development files only; all project task data remains local and is never committed.
9. Undecided technical choices remain explicitly undecided until investigated and approved.


## 15. Implementation status and integration boundary

### Implemented in Todoing

- Project-local SQLite store initialization through the CLI and JSON API.
- User-configurable TOML statuses and transitions.
- Task lifecycle, dependencies, archive/restore, immutable versions, approval baselines, and append-only history.
- Exclusive task locks with heartbeat, release, recovery, and fencing generations.
- Versioned JSON API v1, non-destructive legacy migration, and verified SQLite snapshots.

### Deferred or consumer-owned

- Generic lifecycle events/hooks are deferred; the SQLite backend does not execute the legacy Markdown completion hooks.
- Manifest or another consumer owns orchestration, role policy, OpenSpec workflow, Git workflow, and cross-component consistency checks.
- Integrations are optional. A consumer runs checks only for components it has connected and configured.
- Todoing stores Git commit references for traceability; it does not treat them as proof of completion or enforce Git policy.
- Status mapping, consumer trust/authentication, and event-delivery semantics require explicit decisions at the integration boundary; Todoing must not invent them.
