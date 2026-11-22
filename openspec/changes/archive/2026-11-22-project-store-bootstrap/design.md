# Design: Project store bootstrap

## Context

The storage layer already provides Store::initialize(root, project_id) and Store::open(root, expected_project_id). The public CLI has no new-project initialization command, and API v1 has no store.init operation. Existing API requests load configuration and open the database before dispatch, so initialization must be dispatched before that path.

The selected implementation baseline defines one SQLite database per project, TOML status configuration, and a generated project ID for human CLI initialization when no ID is supplied. Manifest supplies its stable project ID through the JSON API. Todoing remains independent of Manifest and does not infer consumer identity or policy.

## Contract

### CLI

todoing init [--project-id <id>]

- A supplied ID is validated through ProjectId.
- When omitted, Todoing generates a random 128-bit ID with a p- prefix.
- The command initializes the current project root.
- It creates generic default TOML configuration only when no configuration exists.
- It validates and preserves an existing configuration byte-for-byte.
- It refuses to initialize over an existing database or legacy Markdown task data.
- Success reports project ID, schema version, and API version.

### JSON API

Add store.init to API v1.

- The request envelope's required project_id is the ID to initialize; actor_id remains required by the common envelope.
- args is an empty object.
- The operation is dispatched before loading existing configuration or opening the existing store.
- Success returns project_id, schema_version, and api_version.
- A subsequent store.info request verifies the same project ID through the normal open path.
- A repeated or losing concurrent initialization returns store_already_initialized.

## Configuration and filesystem behavior

- If .todoing/config.toml exists, parse and validate it before creating the database; preserve its bytes.
- If it does not exist, serialize ProjectConfig::default() and install it with a temporary file and no-clobber rename.
- If another initializer wins the config-file race, read and validate the winning file; never overwrite it.
- Invalid existing configuration aborts initialization before database activation.
- Configuration creation and database initialization are separate filesystem operations. A generated configuration may remain after a later database initialization failure and is safe to reuse on retry.
- Database creation uses the existing temporary-database and no-clobber activation in Store::initialize; at most one concurrent initializer can activate the database.
- If legacy Markdown task files or .todoing/index.json exist, initialization refuses and directs the caller to todoing migrate. It must not hide legacy tasks behind an empty database.
- The project-local .todoing directory may already exist for configuration or hooks; that alone is not a reason to refuse initialization.

## Failure and compatibility

- Existing stores are never overwritten, migrated, or rebound by init.
- Existing API v1 operations retain their current semantics.
- No Manifest-specific statuses, event handlers, or orchestration logic are installed.
- CLI-generated IDs are not authentication credentials; API callers must supply the expected project ID on every request.

## Test-first verification

1. CLI parsing: optional project ID, generated ID, and invalid supplied IDs.
2. New project: default config and schema are created; returned ID matches database metadata.
3. Existing valid config: preserved byte-for-byte and used to initialize.
4. Invalid config: no active database is created.
5. Existing database and legacy task data: refused without destructive changes.
6. Concurrent initialization: config/database no-clobber behavior and stable conflict.
7. API transport: one response, request-ID echo, stable conflict/config errors and exit codes.
8. Windows and Linux CI: formatting, tests, release build, install, and binary smoke tests.
