# MVP task core

## Why

The current Rust port provides basic task files and a CLI, but it does not yet satisfy the accepted product baseline for reliable agent task management. In particular, it lacks configurable workflows, dependencies, immutable history and versions, exclusive task locks, transactional multi-record updates, and a stable versioned machine interface.

This change establishes normative target requirements for the MVP. It does not claim that these capabilities are implemented.

## Product decisions in scope

- Todoing remains a standalone, local-first Rust CLI with one task store per project.
- Task identifiers are stable and never reused; physical task deletion is not supported.
- Users control workflow statuses and permitted transitions.
- Dependencies, immutable operation history, task versions, exclusive locks, and transactional persistence are MVP capabilities.
- The CLI is the primary interface. Its machine-readable JSON contract is versioned and stable.
- The current Markdown/YAML task files and JSON index are migration inputs, not a constraint that the future transactional store must retain them as its primary format.
- SQLite remains a preliminary candidate only. Storage selection requires separate technical evaluation.

## Goals

- Define observable requirements for the MVP task model and lifecycle.
- Define integrity and concurrency guarantees independently of the eventual storage engine.
- Specify the minimum machine contract required by humans, agents, and external consumers.
- Require a safe migration path from the current repository format.
- Keep technical choices open where the product decisions do not settle them.

## Out of scope

- Manifest orchestration, role policy, OpenSpec workflow, and Git branch/merge policy.
- Manifest-specific statuses, completion gates, or handlers.
- Generic lifecycle events/hooks beyond the current compatibility baseline; these remain post-MVP.
- Archive/restore and automatic archival policies; these remain post-MVP.
- Full import/export and backup automation; these remain post-MVP.
- A graphical interface or central multi-project service.

## Design constraints and unresolved decisions

The requirements deliberately do not select a database, schema, status-configuration syntax, lock implementation, version snapshot format, JSON envelope, or migration mechanics. These require technical research and architecture decisions.

Completion and cancellation must remain distinguishable from workflow status. The exact relationship between outcome values and configurable statuses remains open and must be resolved in the integration design without conflating the concepts.

## Capabilities

- `task-identity-and-lifecycle`: stable task identity and configurable status workflows.
- `task-dependencies`: dependency relationships and cycle detection.
- `task-history-and-versions`: immutable operation history and version snapshots.
- `transactional-storage`: atomic logical mutations and concurrency integrity.
- `task-locking`: exclusive work-session locks and recovery.
- `cli-json-contract`: stable versioned machine interface.
- `legacy-data-migration`: safe migration from Markdown/YAML and JSON storage.

## Completion criteria

The MVP is not complete until every requirement in these capability deltas has an implemented, independently verifiable acceptance test. Architecture decisions and implementation status must remain separately tracked.
