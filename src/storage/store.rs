use super::StoreError;
use crate::domain::ProjectId;
use chrono::{SecondsFormat, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
use tempfile::NamedTempFile;

const SCHEMA_VERSION: u32 = 1;
const BUSY_TIMEOUT: Duration = Duration::from_secs(10);
const INITIAL_SCHEMA: &str = include_str!("../../migrations/0001_initial.sql");

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IntegrityReport {
    pub integrity_check: String,
    pub foreign_key_violations: usize,
    pub schema_version: u32,
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
        configure_connection(&connection)?;
        connection.execute_batch("BEGIN IMMEDIATE;")?;
        connection.execute_batch(INITIAL_SCHEMA)?;

        let created_at = Utc::now().to_rfc3339_opts(SecondsFormat::Micros, true);
        connection.execute(
            "INSERT INTO store_metadata(key, value) VALUES ('schema_version', ?1)",
            params![SCHEMA_VERSION.to_string()],
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
            "INSERT INTO schema_migrations(version, checksum, applied_at) VALUES (?1, ?2, ?3)",
            params![SCHEMA_VERSION, schema_checksum(), created_at],
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

        let connection = connect(&database_path)?;
        let project_id_raw = metadata_value(&connection, "project_id")?;
        let project_id = ProjectId::new(project_id_raw.clone())
            .map_err(|_| StoreError::InvalidMetadata("project_id".into()))?;
        if let Some(expected) = expected_project_id {
            if expected != &project_id {
                return Err(StoreError::ProjectIdMismatch {
                    expected: expected.as_str().to_owned(),
                    actual: project_id_raw,
                });
            }
        }
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

fn connect(path: &Path) -> Result<Connection, StoreError> {
    let connection = Connection::open(path)?;
    configure_connection(&connection)?;
    Ok(connection)
}

fn configure_connection(connection: &Connection) -> Result<(), StoreError> {
    connection.busy_timeout(BUSY_TIMEOUT)?;
    connection.execute_batch("PRAGMA foreign_keys = ON; PRAGMA synchronous = FULL;")?;

    let journal_mode: String =
        connection.query_row("PRAGMA journal_mode = DELETE", [], |row| row.get(0))?;
    if !journal_mode.eq_ignore_ascii_case("delete") {
        return Err(StoreError::InvalidMetadata("journal_mode".into()));
    }

    let foreign_keys: i64 =
        connection.query_row("PRAGMA foreign_keys", [], |row| row.get(0))?;
    if foreign_keys != 1 {
        return Err(StoreError::InvalidMetadata("foreign_keys".into()));
    }

    let synchronous: i64 =
        connection.query_row("PRAGMA synchronous", [], |row| row.get(0))?;
    if synchronous != 2 {
        return Err(StoreError::InvalidMetadata("synchronous".into()));
    }

    let busy_timeout: i64 =
        connection.query_row("PRAGMA busy_timeout", [], |row| row.get(0))?;
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

fn validate_schema(connection: &Connection) -> Result<(), StoreError> {
    let version_raw = metadata_value(connection, "schema_version")?;
    let version = version_raw
        .parse::<u32>()
        .map_err(|_| StoreError::InvalidMetadata("schema_version".into()))?;
    if version != SCHEMA_VERSION {
        return Err(StoreError::UnsupportedSchemaVersion(version));
    }

    let checksum: Option<String> = connection
        .query_row(
            "SELECT checksum FROM schema_migrations WHERE version = ?1",
            params![SCHEMA_VERSION],
            |row| row.get(0),
        )
        .optional()?;
    if checksum.as_deref() != Some(schema_checksum().as_str()) {
        return Err(StoreError::MigrationChecksumMismatch {
            version: SCHEMA_VERSION,
        });
    }

    Ok(())
}

fn schema_checksum() -> String {
    hex::encode(Sha256::digest(INITIAL_SCHEMA.as_bytes()))
}
