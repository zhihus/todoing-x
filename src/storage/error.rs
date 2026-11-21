use std::{error::Error, fmt, io, path::PathBuf};

#[derive(Debug)]
pub enum StoreError {
    AlreadyExists(PathBuf),
    NotInitialized,
    TaskNotFound(i64),
    VersionNotFound { task_id: i64, version_no: i64 },
    InvalidTask(String),
    HistoryRecordFailed(String),
    InvalidConfig(String),
    InvalidStatusTransition(String),
    TaskTerminal,
    DependencyNotFound(i64),
    DependencyCycle,
    DependencyNotFound(i64),
    DependencyCycle,
    ProjectIdMismatch { expected: String, actual: String },
    UnsupportedSchemaVersion(u32),
    MigrationChecksumMismatch { version: u32 },
    IntegrityCheckFailed(String),
    ForeignKeyViolations(usize),
    MissingMetadata(String),
    InvalidMetadata(String),
    Sqlite(rusqlite::Error),
    Io(io::Error),
}

impl fmt::Display for StoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyExists(path) => write!(formatter, "Todoing store already exists: {}", path.display()),
            Self::NotInitialized => formatter.write_str("Todoing store is not initialized"),
            Self::TaskNotFound(id) => write!(formatter, "Task #{id} was not found"),
            Self::VersionNotFound { task_id, version_no } => write!(formatter, "Version {version_no} for task #{task_id} was not found"),
            Self::InvalidTask(message) => write!(formatter, "Invalid task: {message}"),
            Self::HistoryRecordFailed(message) => write!(formatter, "Could not record rejected operation: {message}"),
            Self::InvalidConfig(message) => write!(formatter, "Invalid project configuration: {message}"),
            Self::InvalidStatusTransition(message) => write!(formatter, "Invalid status transition: {message}"),
            Self::TaskTerminal => formatter.write_str("Task has a terminal outcome and must be reopened first"),
            Self::DependencyNotFound(id) => write!(formatter, "Dependency task #{id} was not found"),
            Self::DependencyCycle => formatter.write_str("Dependency would create a cycle"),
            Self::DependencyNotFound(id) => write!(formatter, "Dependency task #{id} was not found"),
            Self::DependencyCycle => formatter.write_str("Dependency would create a cycle"),
            Self::ProjectIdMismatch { expected, actual } => {
                write!(formatter, "Project ID mismatch: expected {expected}, found {actual}")
            }
            Self::UnsupportedSchemaVersion(version) => {
                write!(formatter, "Unsupported Todoing schema version: {version}")
            }
            Self::MigrationChecksumMismatch { version } => {
                write!(formatter, "Checksum mismatch for database migration {version}")
            }
            Self::IntegrityCheckFailed(result) => write!(formatter, "SQLite integrity check failed: {result}"),
            Self::ForeignKeyViolations(count) => {
                write!(formatter, "SQLite reports {count} foreign-key violation(s)")
            }
            Self::MissingMetadata(key) => write!(formatter, "Missing store metadata: {key}"),
            Self::InvalidMetadata(key) => write!(formatter, "Invalid store metadata: {key}"),
            Self::Sqlite(error) => write!(formatter, "SQLite error: {error}"),
            Self::Io(error) => write!(formatter, "Filesystem error: {error}"),
        }
    }
}

impl Error for StoreError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Sqlite(error) => Some(error),
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<rusqlite::Error> for StoreError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sqlite(error)
    }
}

impl From<io::Error> for StoreError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}
