use super::{AppError, ErrorCode};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::{fmt, str::FromStr};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TaskId(i64);

impl TaskId {
    pub fn new(value: i64) -> Result<Self, AppError> {
        if value <= 0 {
            return Err(AppError::new(
                ErrorCode::InvalidRequest,
                "Task ID must be a positive integer",
            ));
        }
        Ok(Self(value))
    }

    pub const fn get(self) -> i64 {
        self.0
    }
}

impl fmt::Display for TaskId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl FromStr for TaskId {
    type Err = AppError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let parsed = value
            .strip_prefix('#')
            .unwrap_or(value)
            .parse::<i64>()
            .map_err(|_| AppError::new(ErrorCode::InvalidRequest, "Task ID must be a positive integer"))?;
        Self::new(parsed)
    }
}

impl Serialize for TaskId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_i64(self.0)
    }
}

impl<'de> Deserialize<'de> for TaskId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = i64::deserialize(deserializer)?;
        Self::new(value).map_err(de::Error::custom)
    }
}
