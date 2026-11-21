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
- Todoing is local-first; Git may version project data, but Git is not the transaction manager or backup system.
- Todoing must preserve data integrity independently of whether the project is under Git.
- Project identity is required at the Manifest integration boundary. The exact mechanism for binding a process to a project is an integration design item, not a Todoing-specific Manifest dependency.

## 4. Task lifecycle and status configuration

- Statuses are configurable by the user; Todoing must not hard-code a single workflow.
- The status configuration is stored in a simple, user-editable project configuration file. Its exact syntax and schema remain an architecture decision.
- Users control the available statuses and their ordering. Permitted transitions must be representable and validated.
- Disabling a status prevents its use for new transitions but must not erase historical records that used it.
- Manifest may publish a recommended status configuration. Todoing remains workflow-agnostic and does not automatically synchronize or impose Manifest statuses.
- The Manifest workflow currently uses: Backlog, Ready, In Progress, In Review, Reviewed, and Done (display labels may be localized by the project).
- Completion and cancellation outcomes must be represented distinctly in the task model. The exact relationship between outcome values and configurable workflow statuses must be settled in the integration design; do not silently conflate them.

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
- The final command set and compatibility policy for the current Python-compatible CLI are architecture decisions.

## 9. Events and hooks

- Todoing may expose generic lifecycle events/hooks for operations such as task creation, update, status transition, archive/restore, and completion of a work session.
- The event mechanism must be generic, documented, and optional. Missing handlers must not break ordinary Todoing operation.
- Todoing does not know what OpenSpec, Manifest, or a particular harness is.
- Consumers such as Manifest may attach their own handlers and enforce consumer-specific policy outside the Todoing core.
- Event payload schema, delivery guarantees, ordering, failure handling, and configuration are architecture decisions to be specified before implementation.

## 10. Git references, backup, and portability

- Tasks may store references to Git commits as traceability metadata.
- Todoing does not enforce project Git workflow or treat a commit as proof that a task is complete.
- Git history is not a substitute for task history, transactional persistence, or backup.
- Backup is the responsibility of the consuming project/framework (Manifest in the intended integration). The design must permit backups to a separate physical disk or remote storage.
- Import/export must preserve tasks, versions, history, dependencies, links, and metadata, and must validate integrity. Implementation is post-MVP.
- Migration from the current Markdown/YAML task files and JSON index must be planned before replacing the current storage format.

## 11. Storage

The storage engine has not been finally selected. SQLite is the preliminary candidate, not an approved final decision. A technical comparison must evaluate transaction semantics, concurrent access, locking, history/version growth, portability, backup/restore, performance, and operational complexity.

The storage choice must support the atomicity and concurrency requirements in this document. Markdown/YAML may remain useful as an import/export or human-readable interchange format even if it is not the primary transactional store.

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
- Archive/restore and configurable archival policies.
- Import/export with full integrity validation.
- Backup automation and advanced diagnostics.
- A read-only multi-project UI, if later justified.

The precise sequencing of post-MVP capabilities may be changed by explicit product decisions.

## 13. Compatibility and migration principles

- The existing Rust implementation is a prototype and compatibility port, not the final product specification.
- Existing commands, task files, and index formats must be inventoried before migration decisions are made.
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
8. Git is useful for traceability but is not a database transaction or backup mechanism.
9. Undecided technical choices remain explicitly undecided until investigated and approved.
