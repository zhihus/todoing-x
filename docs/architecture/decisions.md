# Todoing Architecture Decision Register

**Status:** Consolidated decision baseline  
**Purpose:** Distinguish accepted product decisions from preliminary choices and unresolved design work.  
**Source:** Product and architecture discussions through 21 November 2026, plus the current repository audit.

## 1. Accepted decisions

| Area | Decision |
|---|---|
| Product identity | Todoing is a standalone product and must remain usable without Manifest. |
| Implementation | Todoing is being rewritten/maintained in Rust. |
| Responsibility boundary | Todoing manages tasks only. It does not implement Manifest orchestration, OpenSpec policy, Git workflow, or harness configuration. |
| Integration | Manifest is an external consumer. It uses Todoing's public CLI/JSON interface and owns integration policy and cross-component checks. |
| Project scope | Each project has its own task store. No central multi-project task database in the MVP. |
| Statuses | User-configurable; Manifest may provide a recommended configuration but does not impose or automatically synchronize it. |
| Configuration | Status definitions are kept in .todoing/config.toml, TOML format version 1, with user-defined statuses and transitions. |
| Dependencies | Task dependencies are required; cycles must be detected. |
| History | Full append-only operation history; entries are immutable. |
| Versions | Every successful save creates a version; an approved baseline version is identifiable; versions can be compared. |
| Atomicity | Task mutation, history, version, and associated state updates form one atomic logical operation. |
| Locking | Exclusive database-backed task lock for the full work session. Default lease 300s, heartbeat 60s, recovery grace 60s; SHA-256 token hash, fencing generation, rollback-journal EXCLUSIVE acquisition barrier; non-owner reads are blocked and locked tasks are omitted from lists. |
| Deletion | Physical task deletion is not supported; IDs are stable and never reused. |
| Archive | Archiving is separate from status and completion/cancellation; archive retains history and relationships; restoration is required. |
| Events | Generic optional lifecycle events/hooks may be provided. Todoing must not contain Manifest-specific handlers or policy. |
| JSON interface | API v1: one JSON request per todoing api process, stdin request/stdout response, stable envelope, error codes, and process exit codes. |
| Git | Commit references may be stored for traceability. Git is not the transaction manager, task history, or backup system. |
| Backup | Manifest owns backup destination, retention, and replication. Todoing exposes a consistent SQLite Online Backup API snapshot and verifies it before reporting success. |
| Portability | Legacy migration preserves task IDs and supported fields through a staged, verified, non-destructive database import. Full import/export remains later work. |
| UI | A shared multi-project UI is deferred; if introduced, it is read-only. |
| Agent concurrency | Manifest currently requires sequential agent execution; Todoing still protects against concurrent clients as an independent tool. |

## 2. Manifest integration decisions

- Manifest owns orchestration, role policy, OpenSpec workflow, Git workflow, project governance, and consistency checks.
- Todoing stores task data and exposes generic operations; it does not know about OpenSpec or Manifest.
- Manifest checks consistency among Todoing tasks, OpenSpec artifacts, Git branches/commits, and their relationships.
- Project identity is required at the integration boundary. The project ID is stored in database metadata and required in every Manifest-facing JSON API request; mismatches are rejected.
- Manifest's recommended workflow statuses are Backlog, Ready, In Progress, In Review, Reviewed, and Done. Projects may localize display labels.
- Completion and cancellation are distinct nullable task outcomes. A configured status may map to completed; explicit cancellation sets cancelled without requiring a dedicated status and accepts an optional reason. Workflow status remains independently configurable.
- Todoing hooks are optional; Manifest may use them but must not depend on their presence for core correctness.

## 3. Selected technical decisions

| Area | Selected decision | Remaining verification |
|---|---|---|
| Storage | SQLite + rusqlite bundled; one DB per project; rollback-journal DELETE; synchronous FULL; foreign keys enabled per connection; 10-second busy timeout. | Production repository, durability, migration, and backup tests. |
| Status configuration | TOML format version 1 in .todoing/config.toml. | Parser, validation, and config migration tests. |
| Task locking | Durable lease with 300-second lease, 60-second heartbeat, 60-second recovery grace, token hash, fencing generation, and EXCLUSIVE acquisition barrier. | Clock jumps, sleep/resume, concurrent recovery, and production read-path enforcement. |
| Machine API | JSON API v1, one request per process, stable envelope/error codes/exit codes. | Per-operation schemas and contract tests. |
| Actor identity | Required actor_id; CLI fallback to OS account for humans; explicit actor for agents. | Platform-specific identity lookup and length/format validation. |
| Outcome model | Nullable completed/cancelled outcome separate from workflow status; explicit cancel/reopen operations; optional cancellation reason. | Transition, cancellation, and reopen behavior tests. |
| Migration | Staged DB import, validate before activation, preserve legacy files unchanged. No dual-write. | Windows activation/recovery tests. |
| Backup | Todoing creates and verifies a consistent SQLite snapshot; Manifest owns destination and retention. | Concurrent snapshot test. |
| Human-readable format | Markdown/YAML remains the legacy migration source; not the active storage format. | Optional export format is later work. |

## 4. Open implementation questions

1. Final SQL DDL, indexes, trigger definitions, and schema migration numbering.
2. Exact CLI aliases and deprecation period for legacy commands.
3. Per-operation JSON request/response schemas and generated validation.
4. Clock-jump, machine sleep/resume, and manual lock recovery behavior.
5. Windows same-volume database activation and recovery behavior.
6. Online Backup API behavior while writes continue.
7. Rejected-operation logging behavior when the database itself is unavailable.
8. Structured attachments or application-specific payloads, if a concrete use case appears.
9. Configurable automatic archival policies and retention/cleanup behavior; explicit archive/restore is implemented in schema v2 and exposed through CLI/API.
10. Generic event delivery semantics; deferred beyond MVP.

The selected technical baseline is not to be changed silently. Any deviation requires a new decision record and corresponding tests.

## 5. Delivery boundaries

### MVP

- Task model and configurable statuses/transitions.
- Transactional storage and stable identifiers.
- Dependencies and cycle detection.
- Immutable operation history and task versions.
- Exclusive locks with heartbeat and recovery.
- CLI and versioned JSON interface.
- Safe migration plan from the current repository format.

### Later work

- Generic optional lifecycle events/hooks and Manifest integration.
- Configurable automatic archival policies; explicit archive/restore is implemented.
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
- Selected technical decisions require implementation evidence and must be changed only through an explicit decision record.
- Open questions require an explicit decision record.
- Implementation status must be tracked separately from decision status. A requirement being accepted or documented does not mean it has been implemented.


## 8. Technical research evidence (21 November 2026)

- Detailed candidate comparison and design implications: `docs/architecture/technical-research.md`.
- Architecture overview: `docs/architecture/target-architecture.md`; selected implementation contract: `docs/architecture/implementation-contract.md`.
- The isolated SQLite proof of concept passed all five process-level tests on both Ubuntu and Windows: active-reader barrier, non-owner read exclusion, concurrent ID allocation, rollback after abrupt process termination, and stale-owner fencing.
- The existing application test suite, release build, installation, and binary smoke test also passed on both platforms after restoring missing Clap argument type definitions used by the existing hooks commands.
- These results validate the prototype scenarios only. The production storage layer, full lease protocol, migration, version/history implementation, and API are not yet implemented.
- SQLite, rollback-journal mode, rusqlite bundled, TOML configuration, and JSON API v1 are selected for the MVP implementation baseline.
