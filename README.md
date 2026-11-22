# todoing (Rust)

Rust implementation of [the original Python todoing](https://github.com/bllyanos/todoing), with an explicit migration from Markdown storage to SQLite.

A project without `.todoing/todoing.db` continues to use the legacy Markdown backend. After a successful migration, the SQLite repository becomes authoritative and the human CLI switches to it automatically.

## Install

Requires Rust and Cargo:

```bash
cargo install --git https://github.com/zhihus/todoing-x
```

Verify with `todoing --help` and `todoing version`.

## Build

```bash
cargo build --release
cargo test
```

## Initialize a new project

For a new project without legacy task files, initialize the local SQLite store:

```bash
todoing init
todoing init --project-id my-project
```

The project ID is generated when omitted. Initialization creates the default .todoing/config.toml only when absent, validates and preserves an existing configuration, and refuses to overwrite an existing database. If legacy Markdown task files or index.json exist, use the migration workflow instead.

## Migrate an existing project

First validate the source without changing it:

```bash
todoing migrate --project-id my-project --dry-run
```

Then perform the migration:

```bash
todoing migrate --project-id my-project
```

Migration reads Markdown task files directly. It does not rebuild or rewrite `index.json`, and it retains all source Markdown and index files. It preserves task IDs, status/outcome, body, labels, commit references, and creation timestamps. A compatibility `config.toml` is generated only when one does not already exist.

If a custom `config.toml` exists, it is preserved and must define the four legacy statuses with the expected terminal outcome mappings. Invalid data or a configuration conflict aborts before database activation. The migration report records warnings and SHA-256 hashes of source files.

## CLI

Before migration, the existing Markdown commands remain available. After migration, commands operate on SQLite:

```bash
todoing add "Ship the MVP"
todoing add "Write integration tests" -l testing -s in_progress
todoing ls
todoing ls -n 5
todoing ls -s in_progress
todoing ls -l backend
todoing ls -q auth
todoing ls --archived
todoing archive 4
todoing restore 4
todoing see 4
todoing edit 4
todoing status 4 in_progress
todoing label 4 +p0
todoing body 4 "Replace description"
todoing append 4 "Add details"
todoing commit 4 HEAD
todoing complete 4
todoing cancel 5 --reason "No longer needed"
todoing reopen 5
todoing depends add 6 4
todoing depends list 6
todoing history 4
todoing version 4
todoing verify
todoing snapshot .todoing/snapshots/manual.db
```

Statuses remain `todo`, `in_progress`, `done`, and `cancelled`. Custom status definitions are stored in `.todoing/config.toml`.

Physical deletion is disabled. The legacy `delete` command remains recognized but always rejects deletion; use `cancel` to record a terminal outcome while retaining task history.

`reindex` is a compatibility no-op that verifies SQLite integrity. `ls --scan` is deprecated in SQLite mode.

## Archive and restore

Archiving is an explicit operation independent of workflow status and completion/cancellation. Archived tasks are hidden from the default list, remain inspectable by ID, and retain their history, versions, dependencies, labels, and Git commit references. They are read-only until restored; no automatic archival or physical deletion is performed.

Use `todoing ls --archived` to inspect the archive, `todoing archive <id>` to archive a task, and `todoing restore <id>` to return it to the active list. Archive and restore respect task-lock ownership; archiving does not release an existing lock.

## Actor attribution and locks

Task history records an actor. Set it globally with `--actor` or `TODOING_ACTOR_ID`; the default CLI actor is `cli-user`.

Exclusive task locks use leases and fencing generations:

```bash
todoing lock acquire 4
```

The response contains a bearer `lock_token`. Store it in `TODOING_LOCK_TOKEN` for owner operations; do not pass it as a command-line argument or commit it to Git. Heartbeat, release, and recovery require the owner ID and fencing generation returned by acquire/recover.

## JSON API

The human CLI accepts `--json` before or after a command. In SQLite mode it returns the versioned API v1 envelope:

```bash
todoing --json ls
todoing see 4 --json
todoing complete 4 --json
```

For direct JSON API v1 access, `todoing api` reads one request from stdin and writes one response to stdout:

```bash
printf '%s\n' '{"api_version":1,"project_id":"my-project","actor_id":"agent","operation":"task.list","args":{}}' | todoing api
```

The request and response schemas are in `docs/api/v1/`. Error codes map to stable process exit codes; diagnostics are written to stderr.

## External integrations

Todoing is consumer-agnostic. External orchestrators use project-scoped JSON API v1 and own workflow policy, OpenSpec/Git checks, actor trust, and backup retention. See [the external consumer integration contract](docs/integrations/consumer-contract.md) for bootstrap, project isolation, locks, status configuration, Git references, and current limitations.

## Snapshots

`todoing snapshot <path>` uses SQLite's Online Backup API. It refuses to run while any task lock is active, verifies the resulting database, and returns its size and SHA-256 checksum. Existing destinations are not overwritten unless `--overwrite` is supplied. Do not copy the live database file as a substitute for a verified snapshot.

## Git commit references

`todoing commit 4 HEAD` resolves a Git revision to a full commit SHA. Commit references are stored in SQLite and included in task history/version snapshots.

## Legacy hooks

The optional bundled `on-create`, `before-done-git`, and `before-done-openspec` hooks belong to the legacy Markdown workflow. They are not executed by the SQLite backend; project-specific checks should be performed by the orchestrating agent before invoking lifecycle operations. Hook installation never overwrites an existing file.

## Storage

```
.todoing/
  config.toml
  todoing.db
  migration/
    migration-report-*.json
  tasks/                 # retained legacy source after migration
  index.json             # retained legacy source after migration
  hooks/                 # legacy hook launchers
```

Commit the project configuration and database according to the project's Git policy. Use verified snapshots for portable backups. Keep the migration report with the project history.

## Product direction

The target requirements, accepted decisions, preliminary choices, and open architecture questions are documented in:

- [Product definition](PRODUCT.md)
- [Architecture decision register](docs/architecture/decisions.md)

A feature is not considered complete until its corresponding OpenSpec tasks and cross-platform checks are complete.
