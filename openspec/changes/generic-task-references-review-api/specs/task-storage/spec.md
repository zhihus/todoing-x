## MODIFIED Requirements

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
