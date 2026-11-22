# Design: Project store bootstrap

## Context

The current implementation has `Store::initialize(root, project_id)` in the storage layer and `Store::open(root, expected_project_id)` for existing stores. The public CLI has no `init` command, and API v1 has no `store.init` operation. The API currently reads `.todoing/config.toml` and opens the database before dispatching ordinary operations, so initialization must be handled before that path.

The integration boundary requires Manifest to supply a stable project ID on every API request. Todoing must remain independently usable and must not infer Manifest identity or policy.

## Proposed contract

### CLI

`todoing init --project-id <id>`

- Project ID is explicit and validated using the existing `ProjectId` domain type.
- The command initializes the current project root.
- It creates the default generic TOML status configuration only when no configuration exists.
- It never overwrites an existing configuration or database.
- Success reports the project ID and initialized schema version.

### JSON API

Add `store.init` to API v1.

- The request envelope's `project_id` is the ID to initialize; `actor_id` remains required by the common envelope.
- `args` is an empty object.
- The operation is dispatched before loading the existing configuration or opening the existing store.
- Success returns `project_id`, `schema_version`, and `api_version`.
- A subsequent `store.info` request verifies the same project ID through the normal open path.

### Configuration handling

- If `.todoing/config.toml` exists, parse and validate it before creating the database; preserve its bytes.
- If it does not exist, serialize `ProjectConfig::default()` and install it without clobbering a file created concurrently.
- Invalid existing configuration aborts initialization before the database becomes active.
- Configuration creation and database initialization are separate filesystem operations; failure must leave no active database unless initialization completed successfully. A generated configuration may remain after a later database initialization failure and is safe to reuse on retry.
- Concurrent initializers must not overwrite one another's configuration or database. Exactly one may create the database; other callers receive a stable already-initialized conflict.

## Failure and compatibility

- Existing stores are never overwritten, migrated, or rebound by `init`.
- Supplying a different project ID for an existing store is a mismatch/conflict; the caller must use the store's actual ID or a separate explicit recovery process.
- The operation is not idempotent success: a second initialization attempt returns a stable conflict and does not mutate files.
- Existing API v1 requests and operation semantics remain unchanged.
- No Manifest-specific statuses or event handlers are installed.

## Test-first verification

1. Domain/CLI parsing: required project ID, valid/invalid IDs.
2. New project: default config and schema are created; returned ID matches metadata.
3. Existing valid config: preserved byte-for-byte and used to initialize.
4. Invalid config: no active database is created.
5. Existing database: refused without changing DB or config.
6. Concurrent initialization: no clobber, no partial database/config visible.
7. API transport: one response, request ID echo, stable conflict/config errors and exit codes.
8. Windows and Linux CI: formatting, tests, release build, install, and binary smoke tests.

The exact filesystem coordination primitive and stable error mapping must be finalized in implementation design before coding; tests must exercise the selected behavior on both CI platforms.
