# Project store initialization

## ADDED Requirements

### Requirement: Explicit project store initialization

The system MUST expose an explicit operation to initialize a project-local Todoing store. The operation MUST be available through the human CLI and JSON API v1.

#### Scenario: Initialize a new project through the CLI with a supplied ID

- **WHEN** a user runs todoing init --project-id <id> in a project without an active Todoing database
- **THEN** Todoing creates the project-local SQLite store bound to that ID
- **AND** creates a valid default status configuration only when no configuration file exists
- **AND** reports the initialized project ID

#### Scenario: Initialize a new project through the CLI with a generated ID

- **WHEN** a user runs todoing init without a project ID
- **THEN** Todoing generates a random project ID
- **AND** initializes the project store using that ID
- **AND** reports the generated ID

#### Scenario: Initialize a new project through JSON API

- **WHEN** a client submits store.init with a project ID in the API v1 request envelope
- **THEN** Todoing initializes the project store using that ID
- **AND** returns the project ID and schema/API version information
- **AND** emits exactly one API v1 response

### Requirement: Refuse initialization over legacy task data

Initialization MUST refuse to create a new empty database when legacy Markdown task files or .todoing/index.json are present. The caller MUST be directed to the migration operation.

#### Scenario: Legacy task data exists

- **WHEN** initialization is requested while legacy Markdown task files or .todoing/index.json exist
- **THEN** initialization returns the stable migration_required error
- **AND** no database is created

### Requirement: Preserve existing project configuration

Initialization MUST preserve an existing .todoing/config.toml file. If it exists, Todoing MUST validate it before creating the database and MUST NOT rewrite it. If it does not exist, Todoing MUST install the generic default configuration without overwriting a file created concurrently.

#### Scenario: Reuse valid existing configuration

- **WHEN** a valid project configuration exists before initialization
- **THEN** Todoing uses that configuration
- **AND** the file remains byte-for-byte unchanged

#### Scenario: Reject invalid configuration

- **WHEN** the existing project configuration is invalid
- **THEN** initialization fails with the stable config_invalid error
- **AND** no active database is created

### Requirement: Refuse destructive reinitialization

Initialization MUST NOT overwrite an existing database, change its project ID, or silently treat a different project ID as equivalent.

#### Scenario: Initialize an already initialized project

- **WHEN** a client invokes initialization while the project database already exists
- **THEN** Todoing returns the stable store_already_initialized conflict
- **AND** leaves the database and configuration unchanged

#### Scenario: Concurrent initialization

- **WHEN** multiple clients attempt to initialize the same project concurrently
- **THEN** at most one client creates the active database
- **AND** no client overwrites another client's configuration or database
- **AND** losing clients receive store_already_initialized
