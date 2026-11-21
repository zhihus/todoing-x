# Legacy data migration

## ADDED Requirements

### Requirement: Migrate the current task store without silent data loss

Todoing MUST provide a migration path from the current project-local Markdown task files and JSON index to the target storage model before replacing the current format.

Migration MUST preserve recoverable task identifiers, titles, creation timestamps, statuses, labels, description bodies, and Git commit references.

#### Scenario: Migrate an existing project task store

- **WHEN** a project contains a valid current-format Todoing task store and the user initiates migration
- **THEN** Todoing migrates the task data into the target store
- **AND** preserves the supported task data and identifiers
- **AND** validates the migrated data's integrity

### Requirement: Protect source data during migration

Migration MUST NOT silently delete or overwrite the source task files or index.

#### Scenario: Migration fails

- **WHEN** migration fails or integrity validation detects a mismatch
- **THEN** the original source task store remains available
- **AND** the migration reports the failure without claiming success

### Requirement: Make migration compatibility explicit

Migration MUST report unsupported, malformed, or non-representable source data rather than silently discarding it.

#### Scenario: Encounter unsupported source data

- **WHEN** migration encounters source data that cannot be represented in the target model
- **THEN** it reports the affected data and the reason it cannot be migrated
- **AND** leaves the source data unchanged
