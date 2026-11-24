## ADDED Requirements

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
