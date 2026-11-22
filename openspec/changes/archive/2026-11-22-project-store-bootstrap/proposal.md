# Proposal: Explicit project store initialization

## Problem

The SQLite store can be initialized by the Rust storage layer, but the public CLI and JSON API do not expose a bootstrap operation. Manifest therefore has no supported public contract for creating a new project store before it can call API v1 operations. Every API request already requires a project ID, but the current API can only validate that ID against an existing store.

## Intended change

Add an explicit, non-destructive project-store initialization operation:

- Human CLI: `todoing init --project-id <id>`.
- JSON API v1: `store.init`, using the request envelope's `project_id`.
- Initialize the SQLite store and a valid project status configuration.
- Preserve an existing configuration file; validate it before creating the store.
- Refuse to overwrite an existing database or silently adopt a different project ID.
- Return the initialized project ID and schema/API versions.

This is a generic Todoing capability. It does not add Manifest-specific logic, workflow statuses, task/OpenSpec conventions, or harness integration.

## Out of scope

- Automatic project-ID generation or discovery.
- Central multi-project storage.
- Manifest orchestration or consistency checks.
- Generic lifecycle events/hooks.
- Automatic status synchronization with Manifest.
- Backup destinations, retention, and replication.

## Acceptance

The change is complete only when the CLI and JSON API expose the same initialization semantics, failure cases are stable, existing user configuration and databases are preserved, and Windows/Linux tests pass.
