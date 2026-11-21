# Task CLI

## Purpose

Describe the user-visible commands and machine-readable output currently provided by the Todoing CLI, including the legacy Markdown backend and the migrated SQLite backend.

## Requirements

### Requirement: Select the project task backend

The CLI MUST use the legacy Markdown backend when the project has no active `.todoing/todoing.db`. After explicit migration activates that database, the CLI MUST use SQLite as the authoritative task store.

#### Scenario: Use an unmigrated project

- **WHEN** a task command is run in a project without `.todoing/todoing.db`
- **THEN** the CLI uses the Markdown task files and JSON index

#### Scenario: Use a migrated project

- **WHEN** a task command is run in a project with an active `.todoing/todoing.db`
- **THEN** the CLI uses SQLite
- **AND** does not treat retained Markdown files or `index.json` as the current task state

### Requirement: Create tasks

The CLI MUST provide an `add` command that creates a task with a title and optional labels, status, and description body.

A title and every supplied label MUST contain a non-whitespace character. In the legacy backend, accepted statuses are `todo`, `in_progress`, `done`, and `cancelled`, case-insensitively. In the SQLite backend, the initial status MUST be enabled in the project's `.todoing/config.toml`; it defaults to the first enabled status when not explicitly supplied.

#### Scenario: Create a valid task

- **WHEN** the user creates a task with a valid title and supported optional values
- **THEN** the system assigns a stable identifier and creation timestamp
- **AND** persists the task in the selected backend

#### Scenario: Reject invalid task input

- **WHEN** the title, a supplied label, or the selected status is invalid
- **THEN** the command fails
- **AND** the invalid task is not persisted

### Requirement: List and filter tasks

The CLI MUST provide an `ls` command that lists tasks in descending task-ID order by default. The `--reverse` option MUST list tasks in ascending task-ID order.

The command MUST support a result limit, status filter, one or more label filters, and a case-insensitive query. Query matching MUST use substring matching for title, body, and labels, and exact matching for status. Multiple label filters MUST use AND semantics.

#### Scenario: Filter by multiple labels

- **WHEN** the user supplies more than one label filter
- **THEN** only tasks containing every requested label are returned
- **AND** label comparison is case-insensitive

#### Scenario: Search task fields

- **WHEN** the user supplies a query
- **THEN** the CLI performs case-insensitive substring matching against title, body, and labels
- **AND** matches status only when the query equals the complete status value

#### Scenario: Limit results

- **WHEN** the user supplies a positive result limit
- **THEN** the sorted and filtered result is truncated to that number of tasks

#### Scenario: Scan legacy task files

- **WHEN** the user runs `todoing ls --scan` before migration
- **THEN** the system rebuilds `.todoing/index.json` from Markdown task files before listing

#### Scenario: Reject scan after migration

- **WHEN** the user runs `todoing ls --scan` after SQLite migration
- **THEN** the command rejects the deprecated option
- **AND** does not read retained Markdown files as current task data

### Requirement: Inspect and edit task content

The CLI MUST provide `see`, `edit`, `body`, `append`, `status`, and `label` commands for existing tasks. Task commands MUST accept a numeric task identifier, optionally prefixed with `#`.

In the legacy backend, `see` displays the Markdown task and `edit` opens its Markdown file in the configured editor. In SQLite mode, `see` renders a Markdown representation and `edit` edits a temporary Markdown representation; validated changes are persisted through the SQLite repository.

#### Scenario: Show a task

- **WHEN** the user runs `todoing see` with an existing task ID
- **THEN** the task is displayed in the supported Markdown representation

#### Scenario: Edit a task

- **WHEN** the user runs `todoing edit` with an existing task ID
- **THEN** the task is opened in the configured editor
- **AND** accepted changes are validated and persisted through the selected backend

#### Scenario: Replace or append task body

- **WHEN** the user runs `todoing body` or `todoing append`
- **THEN** the task body is replaced or appended respectively
- **AND** the updated task is persisted

### Requirement: Manage task lifecycle

The CLI MUST provide `complete`, `cancel`, and `reopen` operations in SQLite mode. Cancellation MUST accept an optional reason and MUST NOT require a dedicated workflow status.

#### Scenario: Complete a task

- **WHEN** the user completes an active task
- **THEN** its outcome becomes `completed`
- **AND** its workflow status is retained

#### Scenario: Cancel a task

- **WHEN** the user cancels an active task with or without a reason
- **THEN** its outcome becomes `cancelled`
- **AND** its workflow status is retained

#### Scenario: Reopen a terminal task

- **WHEN** the user reopens a completed or cancelled task
- **THEN** its terminal outcome is cleared
- **AND** any requested status transition is validated against project configuration

### Requirement: Prohibit physical task deletion

The supported CLI MUST NOT physically delete task records. The legacy `delete` command remains recognized for compatibility but MUST reject deletion in both backends. Users MUST use cancellation when they need to record that a task is no longer active.

#### Scenario: Request physical deletion

- **WHEN** the user runs `todoing delete`, with or without `--force`
- **THEN** the command fails with a machine-readable error when JSON output is requested
- **AND** the task and its related records remain intact

### Requirement: Manage dependencies, history, and versions

The SQLite CLI MUST provide commands to add, remove, and list task dependencies; inspect operation history; and inspect task versions.

#### Scenario: Manage dependencies

- **WHEN** the user invokes `todoing depends add|remove|list`
- **THEN** the requested dependency operation is applied or its current relationships are returned

#### Scenario: Inspect history and versions

- **WHEN** the user invokes `todoing history` or `todoing version <task-id>`
- **THEN** the CLI returns the task's recorded operation history or immutable versions

### Requirement: Manage task locks

The SQLite CLI MUST provide lock acquisition, heartbeat, release, and recovery commands. Raw lock tokens MUST NOT be accepted as command-line arguments; owner operations obtain the token from `TODOING_LOCK_TOKEN`.

#### Scenario: Acquire a task lock

- **WHEN** the user acquires a task lock
- **THEN** the response provides the owner ID, bearer token, and fencing generation

#### Scenario: Perform an owner operation

- **WHEN** the user performs heartbeat or release with the matching owner ID, fencing generation, and token environment variable
- **THEN** the lock service validates the current lease and ownership before applying the operation

### Requirement: Migrate, verify, and snapshot the store

The CLI MUST provide explicit `migrate`, `verify`, and `snapshot` commands in addition to `config` inspection.

#### Scenario: Preview or perform migration

- **WHEN** the user invokes `todoing migrate --project-id` with or without `--dry-run`
- **THEN** the command returns a migration report
- **AND** a dry run does not activate a database or modify legacy task sources

#### Scenario: Verify the SQLite store

- **WHEN** the user runs `todoing verify`
- **THEN** the store's integrity and foreign-key constraints are checked

#### Scenario: Create a verified snapshot

- **WHEN** the user runs `todoing snapshot` with an output path
- **THEN** a verified SQLite Online Backup snapshot is created
- **AND** its path, size, and SHA-256 checksum are returned

### Requirement: Rebuild or verify the task index

Before migration, `reindex` MUST rebuild the JSON index from Markdown task files. In SQLite mode, `reindex` MUST NOT rewrite legacy files and instead performs an integrity-verification compatibility action.

#### Scenario: Reindex the legacy backend

- **WHEN** the user runs `todoing reindex` before migration
- **THEN** the JSON index is rebuilt from valid Markdown task files

#### Scenario: Reindex the SQLite backend

- **WHEN** the user runs `todoing reindex` after migration
- **THEN** SQLite integrity is verified
- **AND** retained Markdown and index files remain unchanged

### Requirement: Link Git commits

The CLI MUST provide a `commit` command to link a Git revision to a task. The revision MUST be resolved to a full commit SHA before storage.

#### Scenario: Link a revision

- **WHEN** the user supplies a valid Git revision for an existing task
- **THEN** the resolved full SHA is stored with the task

### Requirement: Record actor attribution

SQLite task operations MUST record an actor identity. The CLI MUST accept `--actor` or `TODOING_ACTOR_ID`; when neither is supplied, it uses the documented `cli-user` fallback.

#### Scenario: Attribute a CLI operation

- **WHEN** the user performs a task operation in SQLite mode
- **THEN** the operation history records the resolved actor identity

### Requirement: JSON output and JSON API v1

The CLI MUST support a global `--json` option. Legacy mode retains its legacy JSON representation. SQLite mode MUST return the versioned API v1 success or error envelope. JSON responses MUST be written to stdout and diagnostics to stderr.

The `api` command MUST read one JSON request from stdin and write exactly one JSON response to stdout per process. Requests MUST identify API version, project, actor, operation, and arguments. Unsupported versions and invalid requests MUST return stable error codes and documented process exit codes.

#### Scenario: Request task data as JSON

- **WHEN** the user invokes a supported command with `--json`
- **THEN** the command emits its documented machine-readable representation on stdout

#### Scenario: Process one JSON API request

- **WHEN** a client submits one valid API v1 request to `todoing api`
- **THEN** the process emits exactly one versioned JSON response
- **AND** echoes `request_id` when supplied
- **AND** keeps diagnostics on stderr

### Requirement: Keep legacy hooks scoped to the legacy backend

The optional `on-create` and completion hooks MUST remain available for the legacy Markdown workflow. SQLite mode MUST NOT execute these legacy hooks.

#### Scenario: Run a legacy hook

- **WHEN** a legacy task operation reaches an installed hook
- **THEN** the hook receives the task JSON on stdin and its exit status controls acceptance

#### Scenario: Do not run a legacy hook in SQLite mode

- **WHEN** a task operation is performed after migration
- **THEN** the legacy hook launchers are not invoked automatically
