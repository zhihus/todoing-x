# Legacy data migration

## Requirements

### Requirement: Migrate existing task records

The system MUST provide a safe migration path from the current Markdown task files with YAML frontmatter and the JSON index.

Migration MUST preserve existing task identifiers and all supported task data, including titles, creation timestamps, statuses, labels, description bodies, and Git commit references.

#### Scenario: Migrate a valid existing task store

- **WHEN** the user migrates a valid current-format task store
- **THEN** each task is represented in the new store with its original identifier and supported data
- **AND** task relationships or metadata that cannot be represented are reported rather than silently discarded

### Requirement: Validate migration input and result

The migration process MUST validate source records and the integrity of the migrated task store.

The process MUST report malformed or unsupported source records and MUST NOT silently treat an incomplete migration as successful.

#### Scenario: Encounter invalid source data

- **WHEN** migration encounters a malformed or unsupported task record
- **THEN** the issue is reported with enough context to identify the source record
- **AND** the migration result does not claim full success

### Requirement: Preserve source data during migration

Migration MUST NOT physically delete or overwrite the source task files or index as part of the migration operation.

#### Scenario: Complete migration

- **WHEN** migration completes
- **THEN** the original Markdown/YAML files and JSON index remain available for verification or rollback
