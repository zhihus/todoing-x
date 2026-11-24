# Task storage

## Purpose

Describe the task persistence behavior currently implemented by Todoing, including the legacy Markdown backend, explicit migration, and the SQLite backend.

## Requirements

### Requirement: Select and retain the project-local task store

The system MUST store Todoing data under the project's `.todoing/` directory. Before migration, the Markdown task files and JSON index are authoritative. After successful migration, `.todoing/todoing.db` is authoritative.

#### Scenario: Initialize the legacy store

- **WHEN** a task operation requires storage and the project has no active SQLite database
- **THEN** the system uses `.todoing/tasks/` and `.todoing/index.json`

#### Scenario: Open the migrated store

- **WHEN** the project contains an active `.todoing/todoing.db`
- **THEN** the system opens the SQLite store and validates its project identity and schema

#### Scenario: Retain legacy source files

- **WHEN** migration activates SQLite
- **THEN** the original Markdown task files and JSON index remain available
- **AND** SQLite operations do not update them as a secondary source of current task state

### Requirement: Persist legacy Markdown task records

The legacy backend MUST persist each task as a Markdown file under `.todoing/tasks/`, named with its task identifier and the `.md` extension. Task records use YAML frontmatter and a Markdown body. The JSON index is rebuildable from valid task files.

#### Scenario: Read and rewrite a legacy task

- **WHEN** a legacy task file contains valid supported frontmatter fields
- **THEN** the system parses the task fields
- **AND** can rewrite the task in the Todoing Markdown representation

The legacy implementation trims leading and trailing whitespace from the description body when reading and writing a task. Frontmatter fields not represented by the legacy task model are not preserved when the task is rewritten.

#### Scenario: Rebuild the legacy index

- **WHEN** the user runs `todoing reindex` or `todoing ls --scan` before migration
- **THEN** the system scans task Markdown files and rebuilds `.todoing/index.json`

### Requirement: Migrate legacy data non-destructively

The system MUST provide a validated migration path from Markdown/YAML task files and the JSON index to SQLite. Markdown task files are the migration source of truth; the JSON index is hashed for reporting but is not trusted as task data.

Migration MUST preserve supported task IDs, titles, creation timestamps, statuses and their outcomes, labels, bodies, and Git commit references. Migration MUST refuse to overwrite an existing database.

#### Scenario: Validate a migration without mutation

- **WHEN** the user runs migration with `dry_run=true`
- **THEN** source files are validated and hashed
- **AND** no configuration, staging database, report, or active database is created

#### Scenario: Activate a migrated store

- **WHEN** migration validates all source records and imports them into a staging database
- **THEN** the staging database is integrity-checked before activation
- **AND** the active database is installed atomically without replacing an existing database
- **AND** source Markdown and index files remain unchanged

#### Scenario: Reject invalid migration input

- **WHEN** a source task or required legacy status mapping is invalid
- **THEN** migration fails before database activation
- **AND** source task files remain intact

#### Scenario: Preserve user configuration

- **WHEN** a valid project `config.toml` already exists
- **THEN** migration validates and uses it without overwriting it
- **AND** it requires the four legacy status IDs and their expected completion/cancellation outcome mappings

#### Scenario: Generate compatibility configuration

- **WHEN** migration finds no project configuration
- **THEN** it generates a compatibility configuration for the legacy status set
- **AND** does so without overwriting any existing file

#### Scenario: Report migration provenance

- **WHEN** migration completes
- **THEN** it writes a report containing imported task count, warnings, and SHA-256 hashes of source files
- **AND** a report-write failure is reported as a warning without discarding the activated database

### Requirement: Persist SQLite task state transactionally

The SQLite backend MUST persist task state, labels, commit references, dependencies, immutable versions, and operation history. A successful task mutation, its new version, the corresponding history event, and related state MUST become visible atomically.

#### Scenario: Commit a task mutation

- **WHEN** a valid task mutation succeeds
- **THEN** the task state, new version, operation-history entry, and related state become visible together

#### Scenario: Roll back a failed task mutation

- **WHEN** a failure occurs before the logical transaction commits
- **THEN** none of the mutation's task, version, history, or related state changes become visible
- **AND** the previously committed task state remains intact

#### Scenario: Avoid redundant versions for no-op operations

- **WHEN** an accepted task operation does not change task state
- **THEN** the operation is recorded as a successful no-op
- **AND** no new task version is created

### Requirement: Preserve data integrity across concurrent clients

The SQLite backend MUST prevent concurrent clients from silently overwriting committed task changes or allocating duplicate task identifiers.

#### Scenario: Concurrent task creation

- **WHEN** multiple clients create tasks concurrently in the same project store
- **THEN** every committed task receives a unique stable identifier
- **AND** no committed task is silently overwritten

#### Scenario: Verify database integrity

- **WHEN** the user invokes store verification
- **THEN** SQLite integrity and foreign-key constraints are checked
- **AND** schema metadata and project identity are validated

### Requirement: Create verified SQLite snapshots

The system MUST provide SQLite Online Backup snapshots. A snapshot MUST be written to a temporary file in the destination directory, verified before activation, and persisted without overwriting an existing destination unless overwrite is explicitly requested.

A snapshot MUST be rejected while any task lock is active. The destination MUST differ from the active database path.

#### Scenario: Create a snapshot

- **WHEN** a client requests a snapshot to a valid destination
- **THEN** the Online Backup API creates a consistent copy
- **AND** the copy is reopened and checked for project identity, schema, SQLite integrity, and foreign-key integrity
- **AND** the response includes destination path, size, and SHA-256 checksum

#### Scenario: Preserve an existing destination

- **WHEN** the destination already exists and overwrite is false
- **THEN** snapshot creation fails
- **AND** the existing destination remains unchanged

#### Scenario: Reject a snapshot during an active task lock

- **WHEN** any task lock is active
- **THEN** snapshot creation is rejected
- **AND** no snapshot is activated at the destination

### Requirement: Persist generic external references and Review records

The SQLite backend MUST persist generic task external references and append-only Review records. Active reference uniqueness MUST be enforced for task and project scopes. Review identifiers MUST be unique within a task. Task metadata MUST persist `revision_count` and an optional `final_commit_sha`. The schema migration MUST preserve all pre-existing task, version, history, lock, dependency, and archive data.

#### Scenario: Migrate an existing version 2 store

- **WHEN** an existing schema version 2 store is opened
- **THEN** the system applies the extensions migration transactionally
- **AND** preserves all existing task data and relationships
- **AND** verifies the migration checksum and records schema version 3

### Requirement: Migrate SQLite schema without data loss

The store MUST migrate existing schema versions to the new schema through numbered,
checksummed migrations. The migration MUST preserve existing tasks, versions,
operation history, baselines, locks, dependencies, archive metadata, and commit
references.

#### Scenario: Upgrade an existing project store

- **WHEN** an existing schema v2 store is opened by the new Todoing version
- **THEN** migration v3 adds external-reference, Review, revision-count, and final
  commit storage
- **AND** existing task data remains unchanged
- **AND** the migration checksum and schema version are recorded atomically

### Requirement: Verify all required schema objects

Store verification MUST check the new external-reference and Review tables,
indexes, constraints, and migration checksums.

#### Scenario: Verify the extended schema

- **WHEN** store.verify runs against a migrated store
- **THEN** it confirms the required schema objects and migration checksums
- **AND** reports corruption when a required object or checksum is missing

### Requirement: Verify and restore complete SQLite snapshots

The system MUST verify a supplied snapshot's project identity, supported schema,
migration checksums, SQLite integrity, foreign-key integrity, file size, and
SHA-256. It MUST restore the complete store through SQLite Online Backup API,
without replacing the active database file.

Restore MUST reject a snapshot from another project or unsupported schema and
MUST reject active task locks. It MUST acquire an exclusive SQLite lock before
checking lock state and capturing the verified pre-restore safety snapshot under
`.todoing/backups/`, then retain that lock through restore. The safety snapshot
MUST represent the exact database state immediately before mutation. If restore or integrity
verification fails, it MUST attempt rollback from that safety snapshot.

#### Scenario: Verify a supplied snapshot

- **WHEN** a client verifies a snapshot created for the same project
- **THEN** Todoing reports project/schema compatibility, SQLite and foreign-key
  integrity, file size, and SHA-256
- **AND** rejects a corrupted or incompatible snapshot

#### Scenario: Restore a verified snapshot

- **WHEN** a client restores a verified snapshot while no task lock is active
- **THEN** the complete database is restored through SQLite Online Backup API
- **AND** the active database file is not replaced
- **AND** a verified pre-restore safety snapshot is retained

#### Scenario: Reject restore while a task lock is active

- **WHEN** any task lock is active
- **THEN** restore is rejected without changing task data

#### Scenario: Roll back a failed restore

- **WHEN** restore or post-restore integrity verification fails
- **THEN** Todoing attempts to restore the retained safety snapshot
- **AND** reports a rollback failure without deleting the safety snapshot
