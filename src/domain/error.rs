use serde::{Deserialize, Serialize};
use std::{error::Error, fmt};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidRequest,
    UnsupportedApiVersion,
    ProjectIdMismatch,
    ConfigInvalid,
    TaskNotFound,
    TaskTerminal,
    TaskLocked,
    LockExpired,
    StaleLock,
    InvalidStatusTransition,
    DependencyNotFound,
    DependencyCycle,
    VersionNotFound,
    StoreBusy,
    StoreCorrupt,
    MigrationConflict,
    IoError,
    HistoryRecordFailed,
    InternalError,
}

impl ErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
            Self::UnsupportedApiVersion => "unsupported_api_version",
            Self::ProjectIdMismatch => "project_id_mismatch",
            Self::ConfigInvalid => "config_invalid",
            Self::TaskNotFound => "task_not_found",
            Self::TaskTerminal => "task_terminal",
            Self::TaskLocked => "task_locked",
            Self::LockExpired => "lock_expired",
            Self::StaleLock => "stale_lock",
            Self::InvalidStatusTransition => "invalid_status_transition",
            Self::DependencyNotFound => "dependency_not_found",
            Self::DependencyCycle => "dependency_cycle",
            Self::VersionNotFound => "version_not_found",
            Self::StoreBusy => "store_busy",
            Self::StoreCorrupt => "store_corrupt",
            Self::MigrationConflict => "migration_conflict",
            Self::IoError => "io_error",
            Self::HistoryRecordFailed => "history_record_failed",
            Self::InternalError => "internal_error",
        }
    }

    pub const fn exit_code(self) -> i32 {
        match self {
            Self::InvalidRequest
            | Self::UnsupportedApiVersion
            | Self::ProjectIdMismatch
            | Self::ConfigInvalid => 2,
            Self::TaskNotFound | Self::DependencyNotFound | Self::VersionNotFound => 3,
            Self::TaskTerminal
            | Self::TaskLocked
            | Self::LockExpired
            | Self::StaleLock
            | Self::InvalidStatusTransition
            | Self::DependencyCycle => 4,
            Self::StoreBusy | Self::StoreCorrupt | Self::IoError | Self::HistoryRecordFailed => 5,
            Self::MigrationConflict => 6,
            Self::InternalError => 70,
        }
    }

    pub const fn is_retryable(self) -> bool {
        matches!(self, Self::TaskLocked | Self::StoreBusy)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppError {
    code: ErrorCode,
    message: String,
}

impl AppError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub const fn code(&self) -> ErrorCode {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub const fn exit_code(&self) -> i32 {
        self.code.exit_code()
    }

    pub const fn is_retryable(&self) -> bool {
        self.code.is_retryable()
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for AppError {}
