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
| Configuration | Status definitions are kept in a simple user-editable project configuration file. Exact format/schema is undecided. |
| Dependencies | Task dependencies are required; cycles must be detected. |
| History | Full append-only operation history; entries are immutable. |
| Versions | Every successful save creates a version; an approved baseline version is identifiable; versions can be compared. |
| Atomicity | Task mutation, history, version, and associated state updates form one atomic logical operation. |
| Locking | Exclusive task lock for the full agent work session, including exclusion of reads by other clients; heartbeat and abnormal-exit recovery are required. |
| Deletion | Physical task deletion is not supported; IDs are stable and never reused. |
| Archive | Archiving is separate from status and completion/cancellation; archive retains history and relationships; restoration is required. |
| Events | Generic optional lifecycle events/hooks may be provided. Todoing must not contain Manifest-specific handlers or policy. |
| JSON interface | A stable, documented, versioned machine-readable contract is required, with predictable error codes and exit codes. |
| Git | Commit references may be stored for traceability. Git is not the transaction manager, task history, or backup system. |
| Backup | Backup belongs to the consuming framework/project; Manifest must provide the backup policy and mechanism. |
| Portability | Import/export must eventually preserve tasks, versions, history, dependencies, references, and metadata, with integrity validation. |
| UI | A shared multi-project UI is deferred; if introduced, it is read-only. |
| Agent concurrency | Manifest currently requires sequential agent execution; Todoing still protects against concurrent clients as an independent tool. |

## 2. Manifest integration decisions

- Manifest owns orchestration, role policy, OpenSpec workflow, Git workflow, project governance, and consistency checks.
- Todoing stores task data and exposes generic operations; it does not know about OpenSpec or Manifest.
- Manifest checks consistency among Todoing tasks, OpenSpec artifacts, Git branches/commits, and their relationships.
- Project identity is required at the integration boundary to prevent accidental access to another project's task store. The binding mechanism is not yet selected.
- Manifest's recommended workflow statuses are Backlog, Ready, In Progress, In Review, Reviewed, and Done. Projects may localize display labels.
- Completion and cancellation outcomes must remain distinguishable. Their exact representation alongside configurable workflow statuses needs an explicit integration decision.
- Todoing hooks are optional; Manifest may use them but must not depend on their presence for core correctness.

## 3. Preliminary decisions

| Area | Preliminary position | Required validation |
|---|---|---|
| Primary storage | SQLite is the leading candidate, not a final selection. | Compare transactions, concurrency, locks, history/version growth, portability, backup/restore, performance, and maintenance. |
| Human-readable format | Markdown/YAML may remain an interchange format. | Define import/export and migration requirements after storage design. |
| Implementation sequencing | Core reliability and task model precede integration features. | Refine after architecture and dependency analysis. |

## 4. Open architecture questions

1. Final storage engine and schema.
2. Status configuration file format, schema, validation, ordering, disabled statuses, and transition rules.
3. Exact task schema and representation of structured attachments/application-specific payloads.
4. Stable CLI command set and compatibility policy for existing commands.
5. JSON API versioning, envelopes, error schema, and exit-code mapping.
6. Event names, payload schema, ordering, delivery guarantees, retries, and handler failure semantics.
7. Lock ownership identity, lease/heartbeat protocol, timeout, crash recovery, and filesystem/database behavior.
8. Version snapshot format, diff semantics, approval marker, and actor identity.
9. Archive/restore data model and optional retention/limit policies.
10. Migration from Markdown/YAML plus JSON index, including preservation of IDs and Git references.
11. Manifest project identity binding and enforcement at the integration boundary.
12. Exact mapping of workflow statuses to completion/cancellation outcomes.

These questions must be resolved through technical design and OpenSpec changes. They must not be silently decided during implementation.

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


## 8. Research outcome — preliminary recommendations

The current technical findings are recorded in [Technical Research: Storage, Concurrency, and Configuration](technical-research.md).

| Area | Research recommendation | Decision status |
|---|---|---|
| Primary storage | SQLite via rusqlite with bundled feature; redb 5.x as the strongest alternative. | Preliminary; prototype and explicit decision required. |
| SQLite journal mode | Select only after proving task-lock acquisition/read exclusion across processes. | Open. |
| Task locking | Durable lease state with owner capability, heartbeat, fencing generation, and a tested acquisition barrier. | Design direction; protocol details open. |
| Status configuration | TOML is preferred for user-edited configuration; JSON for the machine interface. | Preliminary; schema open. |
| Backup | Database-aware consistent snapshot; destination and retention remain the consuming project's responsibility. | Requirement accepted; mechanism depends on selected engine. |

No final storage engine, journal mode, configuration syntax, or lock algorithm is approved. The OpenSpec design remains the normative source for the core task-capability change.
