use super::StoreError;
use crate::domain::ProjectId;
use chrono::{SecondsFormat, Utc};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, backup::Backup, params};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};
use tempfile::NamedTempFile;

const SCHEMA_VERSION: u32 = 3;
const BUSY_TIMEOUT: Duration = Duration::from_secs(10);
const INITIAL_SCHEMA: &str = include_str!("../../migrations/0001_initial.sql");
const ARCHIVE_SCHEMA: &str = include_str!("../../migrations/0002_task_archive.sql");
const EXTENSIONS_SCHEMA: &str = include_str!("../../migrations/0003_task_references_reviews.sql");
const REQUIRED_SCHEMA_OBJECTS: &[(&str, &str)] = &[
    ("table", "schema_migrations"),
    ("table", "store_metadata"),
    ("table", "tasks"),
    ("table", "task_labels"),
    ("table", "task_commit_refs"),
    ("table", "task_dependencies"),
    ("table", "task_versions"),
    ("table", "task_baselines"),
    ("table", "operation_history"),
    ("table", "task_locks"),
    ("table", "task_external_references"),
    ("table", "task_reviews"),
    ("index", "idx_tasks_status"),
    ("index", "idx_task_labels_key"),
    ("index", "idx_task_commit_refs_sha"),
    ("index", "idx_tasks_outcome"),
    ("index", "idx_tasks_updated"),
    ("index", "idx_versions_task_no"),
    ("index", "idx_history_task_event"),
    ("index", "idx_dependencies_reverse"),
    ("index", "idx_locks_state_expiry"),
    ("index", "idx_tasks_archive"),
    ("index", "idx_external_ref_task_active"),
    ("index", "idx_external_ref_project_active"),
    ("index", "idx_external_ref_task_history"),
    ("index", "idx_task_reviews_task_record"),
    ("trigger", "tasks_no_delete"),
    ("trigger", "task_versions_no_update"),
    ("trigger", "task_versions_no_delete"),
    ("trigger", "operation_history_no_update"),
    ("trigger", "operation_history_no_delete"),
    ("trigger", "task_locks_no_delete"),
    ("trigger", "tasks_archive_metadata_insert"),
    ("trigger", "tasks_archive_metadata_update"),
    ("trigger", "task_reviews_no_update"),
    ("trigger", "task_reviews_no_delete"),
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct IntegrityReport {
    pub integrity_check: String,
    pub foreign_key_violations: usize,
    pub schema_version: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SnapshotReport {
    pub path: PathBuf,
    pub size_bytes: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SnapshotVerificationReport {
    pub path: PathBuf,
    pub size_bytes: u64,
    pub sha256: String,
    pub integrity: IntegrityReport,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RestoreReport {
    pub restored: IntegrityReport,
    pub safety_snapshot: SnapshotReport,
}

#[derive(Clone, Debug)]
pub struct Store {
    root: PathBuf,
    todoing_dir: PathBuf,
    database_path: PathBuf,
    project_id: ProjectId,
}

impl Store {
    pub fn initialize(root: impl AsRef<Path>, project_id: ProjectId) -> Result<Self, StoreError> {
        let root = root.as_ref().to_path_buf();
        let todoing_dir = root.join(".todoing");
        fs::create_dir_all(&todoing_dir)?;

        let database_path = todoing_dir.join("todoing.db");
        if database_path.exists() {
            return Err(StoreError::AlreadyExists(database_path));
        }

        let temporary_file = NamedTempFile::new_in(&todoing_dir)?;
        let temporary_path = temporary_file.into_temp_path();

        let connection = Connection::open(&temporary_path)?;
        set_journal_mode_delete(&connection)?;
        configure_connection(&connection)?;
        connection.execute_batch("BEGIN IMMEDIATE;")?;
        connection.execute_batch(INITIAL_SCHEMA)?;

        let created_at = Utc::now().to_rfc3339_opts(SecondsFormat::Micros, true);
        connection.execute(
            "INSERT INTO store_metadata(key, value) VALUES ('schema_version', '1')",
            [],
        )?;
        connection.execute(
            "INSERT INTO store_metadata(key, value) VALUES ('project_id', ?1)",
            params![project_id.as_str()],
        )?;
        connection.execute(
            "INSERT INTO store_metadata(key, value) VALUES ('created_at', ?1)",
            params![created_at],
        )?;
        connection.execute(
            "INSERT INTO schema_migrations(version, checksum, applied_at) VALUES (1, ?1, ?2)",
            params![initial_schema_checksum(), created_at],
        )?;
        connection.execute_batch(ARCHIVE_SCHEMA)?;
        connection.execute(
            "UPDATE store_metadata SET value = ?1 WHERE key = 'schema_version'",
            params![SCHEMA_VERSION.to_string()],
        )?;
        connection.execute(
            "INSERT INTO schema_migrations(version, checksum, applied_at) VALUES (2, ?1, ?2)",
            params![archive_schema_checksum(), created_at],
        )?;
        connection.execute_batch(EXTENSIONS_SCHEMA)?;
        connection.execute(
            "UPDATE store_metadata SET value = ?1 WHERE key = 'schema_version'",
            params![SCHEMA_VERSION.to_string()],
        )?;
        connection.execute(
            "INSERT INTO schema_migrations(version, checksum, applied_at) VALUES (3, ?1, ?2)",
            params![extensions_schema_checksum(), created_at],
        )?;
        connection.execute_batch("COMMIT;")?;
        drop(connection);

        match temporary_path.persist_noclobber(&database_path) {
            Ok(()) => {}
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(StoreError::AlreadyExists(database_path));
            }
            Err(error) => return Err(StoreError::Io(error.error)),
        }

        let store = Self {
            root,
            todoing_dir,
            database_path,
            project_id,
        };
        store.verify()?;
        Ok(store)
    }

    pub fn open(
        root: impl AsRef<Path>,
        expected_project_id: Option<&ProjectId>,
    ) -> Result<Self, StoreError> {
        let root = root.as_ref().to_path_buf();
        let todoing_dir = root.join(".todoing");
        let database_path = todoing_dir.join("todoing.db");
        if !database_path.is_file() {
            return Err(StoreError::NotInitialized);
        }

        let mut connection = connect(&database_path)?;
        let project_id_raw = metadata_value(&connection, "project_id")?;
        let project_id = ProjectId::new(project_id_raw.clone())
            .map_err(|_| StoreError::InvalidMetadata("project_id".into()))?;
        if let Some(expected) = expected_project_id
            && expected != &project_id
        {
            return Err(StoreError::ProjectIdMismatch {
                expected: expected.as_str().to_owned(),
                actual: project_id_raw,
            });
        }
        migrate_schema(&mut connection)?;
        drop(connection);

        let store = Self {
            root,
            todoing_dir,
            database_path,
            project_id,
        };
        store.verify()?;
        Ok(store)
    }

    pub fn project_id(&self) -> &ProjectId {
        &self.project_id
    }

    pub fn database_path(&self) -> &Path {
        &self.database_path
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn todoing_dir(&self) -> &Path {
        &self.todoing_dir
    }

    pub(crate) fn connection(&self) -> Result<Connection, StoreError> {
        connect(&self.database_path)
    }

    pub fn snapshot(
        &self,
        output_path: impl AsRef<Path>,
        overwrite: bool,
    ) -> Result<SnapshotReport, StoreError> {
        let requested = output_path.as_ref();
        let requested = if requested.is_absolute() {
            requested.to_path_buf()
        } else {
            self.root.join(requested)
        };
        let parent = requested.parent().ok_or_else(|| {
            StoreError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Snapshot path has no parent directory",
            ))
        })?;
        let file_name = requested.file_name().ok_or_else(|| {
            StoreError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Snapshot path has no file name",
            ))
        })?;
        let canonical_parent = parent.canonicalize()?;
        let destination = canonical_parent.join(file_name);
        let destination = if destination.exists() {
            destination.canonicalize()?
        } else {
            destination
        };
        let active_database = self.database_path.canonicalize()?;
        if destination == active_database {
            return Err(StoreError::InvalidTask(
                "Snapshot destination must differ from the active database".into(),
            ));
        }
        if destination.exists() && !overwrite {
            return Err(StoreError::AlreadyExists(destination));
        }
        if destination.is_dir() {
            return Err(StoreError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Snapshot destination is a directory",
            )));
        }

        let temporary = NamedTempFile::new_in(&canonical_parent)?;
        let temporary_path = temporary.into_temp_path();
        // Hold SQLite's RESERVED write lock during the backup. Repository writes and
        // EXCLUSIVE lock acquisition are blocked, while the separate backup source
        // connection can still read the consistent database snapshot.
        let mut guard = self.connection()?;
        let transaction = guard.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let active_locks: i64 = transaction.query_row(
            "SELECT COUNT(*) FROM task_locks WHERE state = 'active'",
            [],
            |row| row.get(0),
        )?;
        if active_locks > 0 {
            return Err(StoreError::TaskLocked);
        }

        let backup_source = self.connection()?;
        let mut target = Connection::open(&temporary_path)?;
        configure_connection(&target)?;
        {
            let backup = Backup::new(&backup_source, &mut target)?;
            backup.run_to_completion(5, Duration::from_millis(100), None)?;
        }
        drop(target);
        drop(backup_source);
        transaction.commit()?;

        let snapshot_connection = connect(&temporary_path)?;
        verify_connection(&snapshot_connection, &self.project_id)?;
        drop(snapshot_connection);

        if overwrite {
            temporary_path
                .persist(&destination)
                .map_err(|error| StoreError::Io(error.error))?;
        } else {
            temporary_path
                .persist_noclobber(&destination)
                .map_err(|error| {
                    if error.error.kind() == std::io::ErrorKind::AlreadyExists {
                        StoreError::AlreadyExists(destination.clone())
                    } else {
                        StoreError::Io(error.error)
                    }
                })?;
        }

        let metadata = fs::metadata(&destination)?;
        let mut file = File::open(&destination)?;
        let mut digest = Sha256::new();
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            digest.update(&buffer[..read]);
        }
        Ok(SnapshotReport {
            path: destination,
            size_bytes: metadata.len(),
            sha256: hex::encode(digest.finalize()),
        })
    }

    pub fn verify_snapshot(
        &self,
        snapshot_path: impl AsRef<Path>,
    ) -> Result<SnapshotVerificationReport, StoreError> {
        let path = resolve_store_path(&self.root, snapshot_path.as_ref())?;
        let active_database = self.database_path.canonicalize()?;
        if path == active_database {
            return Err(StoreError::InvalidTask(
                "Snapshot verification requires a separate snapshot file".into(),
            ));
        }
        let connection = Connection::open(&path)?;
        connection.busy_timeout(BUSY_TIMEOUT)?;
        let integrity = verify_connection(&connection, &self.project_id)?;
        drop(connection);
        let (size_bytes, sha256) = file_digest(&path)?;
        Ok(SnapshotVerificationReport {
            path,
            size_bytes,
            sha256,
            integrity,
        })
    }

    pub fn restore_snapshot(
        &self,
        snapshot_path: impl AsRef<Path>,
        expected_sha256: Option<&str>,
    ) -> Result<RestoreReport, StoreError> {
        let source_path = resolve_store_path(&self.root, snapshot_path.as_ref())?;
        let active_database = self.database_path.canonicalize()?;
        if source_path == active_database {
            return Err(StoreError::InvalidTask(
                "Restore source must differ from the active database".into(),
            ));
        }

        // Lock and verify the exact source connection that will be restored.
        // This prevents a snapshot file from being replaced or modified between
        // checksum verification and the Online Backup API copy.
        let source = Connection::open(&source_path)?;
        source.busy_timeout(BUSY_TIMEOUT)?;
        source.execute_batch("PRAGMA locking_mode = EXCLUSIVE;")?;
        verify_connection(&source, &self.project_id)?;
        let (_, source_sha256) = file_digest(&source_path)?;
        if let Some(expected) = expected_sha256
            && (!expected.eq_ignore_ascii_case(&source_sha256)
                || expected.len() != 64
                || !expected.bytes().all(|byte| byte.is_ascii_hexdigit()))
        {
            return Err(StoreError::InvalidTask("snapshot_checksum_mismatch".into()));
        }

        let mut target = self.connection()?;

        // EXCLUSIVE locking mode retains the database lock for this connection's
        // lifetime after the first read. This lets the same idle connection serve
        // as the Online Backup API source for the safety snapshot, then as the
        // restore destination, without a transaction gap or a second DB handle.
        target.execute_batch("PRAGMA locking_mode = EXCLUSIVE;")?;
        let active_locks: i64 = target.query_row(
            "SELECT COUNT(*) FROM task_locks WHERE state = 'active'",
            [],
            |row| row.get(0),
        )?;
        if active_locks > 0 {
            return Err(StoreError::TaskLocked);
        }

        // Snapshot the exact database state protected by the exclusive lock.
        let backup_dir = self.todoing_dir.join("backups");
        fs::create_dir_all(&backup_dir)?;
        let backup_name = format!(
            "pre-restore-{}-{}.db",
            Utc::now().format("%Y%m%dT%H%M%S%.6fZ"),
            std::process::id()
        );
        let temporary = NamedTempFile::new_in(&backup_dir)?;
        let temporary_path = temporary.into_temp_path();
        let mut safety_target = Connection::open(&temporary_path)?;
        configure_connection(&safety_target)?;
        {
            let backup = Backup::new(&target, &mut safety_target)?;
            backup.run_to_completion(5, Duration::from_millis(100), None)?;
        }
        drop(safety_target);
        let safety_connection = connect(&temporary_path)?;
        verify_connection(&safety_connection, &self.project_id)?;
        drop(safety_connection);
        let (safety_size_bytes, safety_sha256) = file_digest(&temporary_path)?;
        let safety_path = backup_dir.join(backup_name);
        temporary_path
            .persist_noclobber(&safety_path)
            .map_err(|error| {
                if error.error.kind() == std::io::ErrorKind::AlreadyExists {
                    StoreError::AlreadyExists(safety_path.clone())
                } else {
                    StoreError::Io(error.error)
                }
            })?;
        let safety_snapshot = SnapshotReport {
            path: safety_path,
            size_bytes: safety_size_bytes,
            sha256: safety_sha256,
        };

        let restore_result = (|| {
            {
                let backup = Backup::new(&source, &mut target)?;
                backup.run_to_completion(5, Duration::from_millis(100), None)?;
            }
            verify_connection(&target, &self.project_id)
        })();

        let restored = match restore_result {
            Ok(report) => report,
            Err(restore_error) => {
                let rollback_result = (|| {
                    let safety_source = Connection::open(&safety_snapshot.path)?;
                    safety_source.busy_timeout(BUSY_TIMEOUT)?;
                    {
                        let backup = Backup::new(&safety_source, &mut target)?;
                        backup.run_to_completion(5, Duration::from_millis(100), None)?;
                    }
                    verify_connection(&target, &self.project_id)
                })();
                if let Err(rollback_error) = rollback_result {
                    return Err(StoreError::IntegrityCheckFailed(format!(
                        "Snapshot restore failed ({restore_error}); automatic rollback from {} also failed ({rollback_error})",
                        safety_snapshot.path.display()
                    )));
                }
                return Err(restore_error);
            }
        };
        drop(target);
        drop(source);

        Ok(RestoreReport {
            restored,
            safety_snapshot,
        })
    }

    pub fn verify(&self) -> Result<IntegrityReport, StoreError> {
        let connection = self.connection()?;
        let actual_raw = metadata_value(&connection, "project_id")?;
        let actual = ProjectId::new(actual_raw.clone())
            .map_err(|_| StoreError::InvalidMetadata("project_id".into()))?;
        if actual != self.project_id {
            return Err(StoreError::ProjectIdMismatch {
                expected: self.project_id.as_str().to_owned(),
                actual: actual_raw,
            });
        }
        validate_schema(&connection)?;

        let integrity_check: String =
            connection.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        if integrity_check != "ok" {
            return Err(StoreError::IntegrityCheckFailed(integrity_check));
        }

        let mut statement = connection.prepare("PRAGMA foreign_key_check")?;
        let rows = statement.query_map([], |_| Ok(()))?;
        let mut foreign_key_violations = 0;
        for row in rows {
            row?;
            foreign_key_violations += 1;
        }
        if foreign_key_violations != 0 {
            return Err(StoreError::ForeignKeyViolations(foreign_key_violations));
        }

        Ok(IntegrityReport {
            integrity_check,
            foreign_key_violations,
            schema_version: SCHEMA_VERSION,
        })
    }
}

fn resolve_store_path(root: &Path, requested: &Path) -> Result<PathBuf, StoreError> {
    let path = if requested.is_absolute() {
        requested.to_path_buf()
    } else {
        root.join(requested)
    };
    Ok(path.canonicalize()?)
}

fn file_digest(path: &Path) -> Result<(u64, String), StoreError> {
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() {
        return Err(StoreError::InvalidTask(
            "Snapshot path must identify a regular file".into(),
        ));
    }
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok((metadata.len(), hex::encode(digest.finalize())))
}

fn verify_connection(
    connection: &Connection,
    expected_project_id: &ProjectId,
) -> Result<IntegrityReport, StoreError> {
    let actual_raw = metadata_value(connection, "project_id")?;
    let actual = ProjectId::new(actual_raw.clone())
        .map_err(|_| StoreError::InvalidMetadata("project_id".into()))?;
    if &actual != expected_project_id {
        return Err(StoreError::ProjectIdMismatch {
            expected: expected_project_id.as_str().to_owned(),
            actual: actual_raw,
        });
    }
    validate_schema(connection)?;
    let integrity_check: String =
        connection.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if integrity_check != "ok" {
        return Err(StoreError::IntegrityCheckFailed(integrity_check));
    }
    let mut statement = connection.prepare("PRAGMA foreign_key_check")?;
    let rows = statement.query_map([], |_| Ok(()))?;
    let mut foreign_key_violations = 0;
    for row in rows {
        row?;
        foreign_key_violations += 1;
    }
    if foreign_key_violations != 0 {
        return Err(StoreError::ForeignKeyViolations(foreign_key_violations));
    }
    Ok(IntegrityReport {
        integrity_check,
        foreign_key_violations,
        schema_version: SCHEMA_VERSION,
    })
}

fn connect(path: &Path) -> Result<Connection, StoreError> {
    let connection = Connection::open(path)?;
    configure_connection(&connection)?;
    Ok(connection)
}

fn set_journal_mode_delete(connection: &Connection) -> Result<(), StoreError> {
    let journal_mode: String =
        connection.query_row("PRAGMA journal_mode = DELETE", [], |row| row.get(0))?;
    if !journal_mode.eq_ignore_ascii_case("delete") {
        return Err(StoreError::InvalidMetadata("journal_mode".into()));
    }
    Ok(())
}

fn configure_connection(connection: &Connection) -> Result<(), StoreError> {
    connection.busy_timeout(BUSY_TIMEOUT)?;
    connection.execute_batch("PRAGMA foreign_keys = ON; PRAGMA synchronous = FULL;")?;

    let journal_mode: String = connection.query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
    if !journal_mode.eq_ignore_ascii_case("delete") {
        return Err(StoreError::InvalidMetadata("journal_mode".into()));
    }

    let foreign_keys: i64 = connection.query_row("PRAGMA foreign_keys", [], |row| row.get(0))?;
    if foreign_keys != 1 {
        return Err(StoreError::InvalidMetadata("foreign_keys".into()));
    }

    let synchronous: i64 = connection.query_row("PRAGMA synchronous", [], |row| row.get(0))?;
    if synchronous != 2 {
        return Err(StoreError::InvalidMetadata("synchronous".into()));
    }

    let busy_timeout: i64 = connection.query_row("PRAGMA busy_timeout", [], |row| row.get(0))?;
    if busy_timeout != BUSY_TIMEOUT.as_millis() as i64 {
        return Err(StoreError::InvalidMetadata("busy_timeout".into()));
    }

    Ok(())
}

fn metadata_value(connection: &Connection, key: &str) -> Result<String, StoreError> {
    connection
        .query_row(
            "SELECT value FROM store_metadata WHERE key = ?1",
            params![key],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| StoreError::MissingMetadata(key.to_owned()))
}

fn migrate_schema(connection: &mut Connection) -> Result<(), StoreError> {
    let version_raw = metadata_value(connection, "schema_version")?;
    let version = version_raw
        .parse::<u32>()
        .map_err(|_| StoreError::InvalidMetadata("schema_version".into()))?;
    if version > SCHEMA_VERSION {
        return Err(StoreError::UnsupportedSchemaVersion(version));
    }

    if version == 1 {
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Exclusive)?;
        let locked_version = metadata_value(&transaction, "schema_version")?
            .parse::<u32>()
            .map_err(|_| StoreError::InvalidMetadata("schema_version".into()))?;
        if locked_version == 1 {
            verify_migration_checksum(&transaction, 1, initial_schema_checksum())?;
            transaction.execute_batch(ARCHIVE_SCHEMA)?;
            transaction.execute(
                "UPDATE store_metadata SET value = '2' WHERE key = 'schema_version'",
                [],
            )?;
            transaction.execute(
                "INSERT INTO schema_migrations(version, checksum, applied_at) VALUES (2, ?1, ?2)",
                params![
                    archive_schema_checksum(),
                    Utc::now().to_rfc3339_opts(SecondsFormat::Micros, true)
                ],
            )?;
        } else if locked_version > SCHEMA_VERSION {
            return Err(StoreError::UnsupportedSchemaVersion(locked_version));
        }
        transaction.commit()?;
    }

    if version <= 2 {
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Exclusive)?;
        let locked_version = metadata_value(&transaction, "schema_version")?
            .parse::<u32>()
            .map_err(|_| StoreError::InvalidMetadata("schema_version".into()))?;
        if locked_version == 2 {
            verify_migration_checksum(&transaction, 2, archive_schema_checksum())?;
            transaction.execute_batch(EXTENSIONS_SCHEMA)?;
            transaction.execute(
                "UPDATE store_metadata SET value = '3' WHERE key = 'schema_version'",
                [],
            )?;
            transaction.execute(
                "INSERT INTO schema_migrations(version, checksum, applied_at) VALUES (3, ?1, ?2)",
                params![
                    extensions_schema_checksum(),
                    Utc::now().to_rfc3339_opts(SecondsFormat::Micros, true)
                ],
            )?;
        } else if locked_version > SCHEMA_VERSION {
            return Err(StoreError::UnsupportedSchemaVersion(locked_version));
        }
        transaction.commit()?;
    }

    validate_schema(connection)
}

fn verify_migration_checksum(
    connection: &Connection,
    version: u32,
    expected: String,
) -> Result<(), StoreError> {
    let checksum: Option<String> = connection
        .query_row(
            "SELECT checksum FROM schema_migrations WHERE version = ?1",
            params![version],
            |row| row.get(0),
        )
        .optional()?;
    if checksum.as_deref() != Some(expected.as_str()) {
        return Err(StoreError::MigrationChecksumMismatch { version });
    }
    Ok(())
}

fn validate_schema(connection: &Connection) -> Result<(), StoreError> {
    for (object_type, object_name) in REQUIRED_SCHEMA_OBJECTS {
        let exists: i64 = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = ?1 AND name = ?2)",
            params![*object_type, *object_name],
            |row| row.get(0),
        )?;
        if exists != 1 {
            return Err(StoreError::InvalidMetadata(format!(
                "schema object {object_type}:{object_name}"
            )));
        }
    }

    let version_raw = metadata_value(connection, "schema_version")?;
    let version = version_raw
        .parse::<u32>()
        .map_err(|_| StoreError::InvalidMetadata("schema_version".into()))?;
    if version != SCHEMA_VERSION {
        return Err(StoreError::UnsupportedSchemaVersion(version));
    }
    verify_migration_checksum(connection, 1, initial_schema_checksum())?;
    verify_migration_checksum(connection, 2, archive_schema_checksum())?;
    verify_migration_checksum(connection, 3, extensions_schema_checksum())?;
    Ok(())
}

fn initial_schema_checksum() -> String {
    hex::encode(Sha256::digest(INITIAL_SCHEMA.as_bytes()))
}

fn archive_schema_checksum() -> String {
    hex::encode(Sha256::digest(ARCHIVE_SCHEMA.as_bytes()))
}

fn extensions_schema_checksum() -> String {
    hex::encode(Sha256::digest(EXTENSIONS_SCHEMA.as_bytes()))
}
