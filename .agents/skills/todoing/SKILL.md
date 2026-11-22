---
name: todoing
description: Use the Todoing CLI for local project-scoped task management. Todoing stores task state, history, versions, dependencies, and locks in a SQLite database; use its CLI or JSON API rather than editing store files directly.
---

# todoing

Todoing is a local-first Rust task manager. Each project has its own SQLite store under .todoing/todoing.db. The CLI is the primary human/agent interface; JSON API v1 is the stable machine interface.

## Initialize or migrate a project

For a new project with no legacy task data:

```bash
todoing init
todoing init --project-id my-project
```

The project ID is generated when omitted. When called by Manifest, use the stable project ID supplied by Manifest.

For a project that already has legacy Markdown tasks, validate and migrate them instead of initializing an empty store:

```bash
todoing migrate --project-id my-project --dry-run
todoing migrate --project-id my-project
```

Migration preserves legacy source files and imports task IDs, content, statuses/outcomes, labels, commit references, and creation timestamps. Do not run init when .todoing/tasks/*.md or .todoing/index.json contains legacy task data; init deliberately refuses this case.

## Store and configuration

- .todoing/todoing.db is the source of truth for task state, versions, history, dependencies, and locks.
- .todoing/config.toml contains user-controlled status definitions, transitions, and lock settings.
- Existing config.toml is validated and preserved by init; it is not silently replaced.
- Status IDs and transitions are project-configurable. Inspect the project configuration rather than assuming a fixed workflow.
- Physical task deletion is not supported. Use cancel for a terminal outcome or archive to hide a task from the active list. Archive and completion are separate concepts.
- Archived tasks retain history and relationships and can be restored.

## Common commands

### Create and inspect

```bash
todoing add "Set up CI pipeline"
todoing add "Fix API bug" -s in_progress -l bug -b "Reproduce the failure."
todoing ls
todoing ls -n 10
todoing ls -s in_progress
todoing ls -l backend
todoing ls -q "connection timeout"
todoing see 3
```

### Update task content and metadata

```bash
todoing body 3 "Replace the description."
todoing append 3 "Record the next progress update."
todoing status 3 in_progress
todoing label 3 +bug
todoing commit 3 HEAD
```

Commit references are resolved to full Git commit SHAs and stored as traceability metadata. A commit link is not proof that a task is complete.

### Completion, cancellation, and archive

```bash
todoing complete 3
todoing cancel 4 --reason "No longer needed"
todoing reopen 4
todoing archive 5
todoing ls --archived
todoing restore 5
```

Completion/cancellation outcomes are distinct from workflow status. Archiving does not change status or outcome. Physical deletion is rejected.

### Dependencies, history, and versions

```bash
todoing depends add 6 4
todoing depends list 6
todoing history 3
todoing version 3
```

Dependencies are project-local. Todoing rejects self-dependencies and dependency cycles. Successful task saves create immutable versions and append-only operation history.

### Exclusive locks

```bash
todoing lock acquire 3
```

The result includes an owner ID, secret lock token, and fencing generation. Keep the token secret and set TODOING_LOCK_TOKEN for owner operations; never put it in command-line arguments, task content, or Git. Heartbeat, release, and recovery require the owner ID and fencing generation. Non-owner reads and writes of a locked task are rejected; list/search omits locked tasks.

## JSON interface

Use --json for machine-readable CLI output. API v1 processes one JSON request per invocation:

```bash
todoing --json ls
printf '%s\n' '{"api_version":1,"project_id":"my-project","actor_id":"agent","operation":"task.list","args":{}}' | todoing api
```

Every API request includes api_version, project_id, actor_id, operation, and args. request_id is optional and echoed unchanged. The supplied project_id must match the store metadata.

Manifest initializes a project store with the stable ID it owns:

```json
{"api_version":1,"project_id":"my-project","actor_id":"manifest","operation":"store.init","args":{}}
```

Todoing validates and preserves project configuration, creates the store without overwriting an existing database, and returns the initialized project ID and schema version. A repeated initialization returns store_already_initialized.

## Completion policy and hooks

Todoing core does not enforce Manifest or OpenSpec workflow policy. The SQLite backend does not execute the legacy Markdown completion hooks. Manifest is responsible for orchestration, OpenSpec checks, Git policy, user approval, and cross-component consistency.

The bundled hooks commands and scripts belong to the legacy Markdown workflow. Do not rely on them as SQLite task-completion gates.

## Agent operating rules

- Run commands from the project root so Todoing resolves the correct project-local store.
- Use the CLI or JSON API; do not edit SQLite files directly.
- Inspect the configured statuses before changing task status.
- Use a lock for the full duration of an agent work session when exclusive ownership is required.
- Record meaningful progress in the task body using append.
- Do not mark a task complete until the work and the consuming workflow's checks are complete.
- Use verify to check store integrity and snapshot to create a verified SQLite backup.
- Use todoing migrate for legacy Markdown projects; do not use init to replace existing task data.

## Self-dogfooding

When working in the todoing-x repository, inspect the project's tasks with todoing ls before starting work.
