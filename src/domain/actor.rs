use super::{AppError, ErrorCode};
use std::fmt;

const MAX_ACTOR_ID_CHARS: usize = 128;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ActorId(String);

impl ActorId {
    pub fn new(value: impl Into<String>) -> Result<Self, AppError> {
        let value = value.into();
        if value.trim().is_empty() || value.chars().count() > MAX_ACTOR_ID_CHARS {
            return Err(AppError::new(
                ErrorCode::InvalidRequest,
                "Actor ID must contain 1 to 128 non-whitespace characters",
            ));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ActorId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}
