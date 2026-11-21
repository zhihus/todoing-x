# Todoing Architecture Decision Register

**Status:** Consolidated decision baseline  
**Purpose:** Distinguish accepted product decisions from preliminary choices and unresolved design work.  
**Source:** Product and architecture discussions through 21 November 2026, repository audit, and cross-platform storage/lock prototypes.

## 1. Accepted decisions

| Area | Decision |
|---|---|
| Product identity | Todoing is a standalone product and must remain usable without Manifest. |
| Implementation | Todoing is being rewritten/maintained in Rust. |
| Responsibility boundary | Todoing manages tasks only. It does not implement Manifest orchestration, OpenSpec policy, Git workflow, or harness configuration. |
| Integration | Manifest is an external consumer. It uses Todoing's public CLI/JSON interface and owns integration policy and cross-component checks. |
| Project scope | Each project has its own task store. No central multi-project task database in the MVP. |
| Statuses | User-configurable; Manifest may provide a recommended configuration but does not impose or automatically synchronize it. |
| Configuration | Status definitions and permitted transitions are stored in TOML at `.todoing/config.toml`; schema version 1 and the default four-status workflow are defined in the architecture design. |
| Dependencies | Task dependencies are required; cycles must be detected. |
| History | Full append-only operation history; entries are immutable. |
| Versions | Every successful save creates a version; an approved baseline version is identifiable; versions can be compared. |
| Atomicity | Task mutation, history, version, and associated state updates form one atomic logical operation. |
| Locking | Use a persisted task-owner lease in the database plus a short-lived per-task OS file-lock barrier. Ordinary task operations hold a shared barrier; lock acquisition, release, and recovery take an exclusive barrier. Full lease/owner semantics still require implementation tests. |
| Deletion | Physical task deletion is not supported; IDs are stable and never reused. |
| Archive | Archiving is separate from status and completion/cancellation; archive retains history and relationships; restoration is required. |
| Events | Generic optional lifecycle events/hooks may be provided. Todoing must not contain Manifest-specific handlers or policy. |
| JSON interface | A stable, documented, versioned machine-readable contract is required, with predictable error codes and exit codes. |
| Git | Commit references may be stored for traceability. Git is not the transaction manager, task history, or backup system. |
| Backup | Backup belongs to the consuming framework/project; Manifest must provide the backup policy and mechanism. |
| Portability | Import/export must eventually preserve tasks, versions, history, dependencies, references, and metadata, with integrity validation. |
| UI | A shared multi-project UI is deferred; if introduced, it is read-only. |
| Agent concurrency | Manifest currently requires sequential agent execution; Todoing still protects against concurrent clients as an independent tool. |
| Completion/cancellation outcomes | Each configured status maps to no outcome, `completed`, or `cancelled`; the task stores outcome separately. Entering an outcome status sets it, leaving for a non-outcome status clears it, and history retains prior changes. |
| Primary storage | SQLite via rusqlite with the bundled SQLite library is selected. Use WAL mode on a local project filesystem; network filesystems are unsupported. |
| Rust file-lock API | Use the standard-library shared/exclusive file-lock API (stable since Rust 1.89); declare Rust 1.89 as the minimum supported version. |

## 2. Manifest integration decisions

- Manifest owns orchestration, role policy, OpenSpec workflow, Git workflow, project governance, and consistency checks.
- Todoing stores task data and exposes generic operations; it does not know about OpenSpec or Manifest.
- Manifest checks consistency among Todoing tasks, OpenSpec artifacts, Git branches/commits, and their relationships.
- Project identity is required at the integration boundary to prevent accidental access to another project's task store. The binding mechanism is not yet selected.
- Manifest's recommended workflow statuses are Backlog, Ready, In Progress, In Review, Reviewed, and Done. Projects may localize display labels.
- Completion and cancellation outcomes remain distinct; the mapping is defined by each project's status configuration and stored separately from the workflow status.
- Todoing hooks are optional; Manifest may use them but must not depend on their presence for core correctness.

## 3. Preliminary decisions

| Area | Preliminary position | Required validation |
|---|---|---|
| Task lock lease details | The hybrid lease + OS file-lock barrier is selected as the coordination model; lease duration, heartbeat interval, owner credential storage, and stale-lock policy are not finalized. | Implement and test owner authorization, lease expiry, crash recovery, and list/read behavior across separate Windows/Linux processes. See [task lock protocol research](research/task-lock-protocol.md). |
| Human-readable migration format | Existing Markdown/YAML remains a migration input and may remain an interchange format. | Use a maintained YAML parser for legacy frontmatter; validate unknown-field preservation and migration integrity. |
| Implementation sequencing | Core reliability and task model precede integration features. | Refine after architecture and dependency analysis. |

## 4. Open architecture questions

1. Exact task schema fields for structured metadata and future attachment support.
2. Stable CLI command compatibility policy and per-command JSON data schemas.
3. Event names, payload schema, ordering, delivery guarantees, retries, and handler failure semantics.
4. Lock owner identity versus secret credential, credential delivery to later CLI calls, lease duration, heartbeat interval, expiry/recovery behavior, and list/filter behavior for locked tasks.
5. Version comparison semantics, approval history representation, and actor identity details.
6. Archive/restore data model and optional retention/limit policies.
7. Migration conflict policy, rerun behavior, and handling of unknown frontmatter.
8. Manifest project identity binding and enforcement at the integration boundary.
9. Backup/restore procedure and integrity verification for the SQLite store.

These questions must be resolved through technical design and OpenSpec changes. They must not be silently decided during implementation.

## 5. Delivery boundaries

### MVP

- Task model and configurable statuses/transitions.
- Transactional SQLite storage and stable identifiers.
- Dependencies and cycle detection.
- Immutable operation history and task versions.
- Exclusive locks with heartbeat and recovery.
- CLI and versioned JSON interface.
- Safe migration plan from the current repository format.

### Later work

- Generic optional lifecycle events/hooks and Manifest integration.
- Archive/restore and archival policies.
- Full import/export and integrity validation.
- Manifest backup automation.
- Read-only multi-project UI and advanced diagnostics.

## 6. Current repository facts (audit baseline)

At the inspected baseline a008836:

- The implementation is a Rust CLI with Markdown task files, YAML frontmatter, and a rebuildable JSON index.
- Four statuses are hard-coded: todo, in_progress, done, cancelled.
- The README documents atomic replacement for individual task/index files, but this does not establish atomicity of a multi-file logical operation.
- The CLI includes a physical-delete command; this conflicts with the accepted product direction and must be addressed in the redesign.
- The repository includes optional completion hooks and an on-create hook. Their current behavior is implementation detail, not the final generic event contract.
- Dependencies, append-only history, task versions, exclusive task locks, archive/restore, and a versioned JSON API are not implemented in the inspected baseline.
- Existing Git commit references are a useful traceability feature to preserve where possible.

This section records static repository inspection. It does not claim that tests were executed.

## 7. Decision discipline

- Accepted decisions are product constraints unless explicitly superseded.
- Preliminary decisions require validation before becoming final.
- Open questions require an explicit decision record.
- Implementation status must be tracked separately from decision status. A requirement being accepted or documented does not mean it has been implemented.
