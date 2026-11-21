use super::{AppError, ErrorCode};
use std::{fmt, str::FromStr};

const MAX_STATUS_ID_CHARS: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StatusId(String);

impl StatusId {
    pub fn new(value: impl Into<String>) -> Result<Self, AppError> {
        let value = value.into();
        let valid = !value.is_empty()
            && value.chars().count() <= MAX_STATUS_ID_CHARS
            && value.as_bytes().first().is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
            && value.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || byte == b'_'
                    || byte == b'-'
            });

        if !valid {
            return Err(AppError::new(
                ErrorCode::InvalidRequest,
                "Status ID must be a lowercase slug of at most 64 characters",
            ));
        }

        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for StatusId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for StatusId {
    type Err = AppError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}
