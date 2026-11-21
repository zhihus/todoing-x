use crate::domain::TaskOutcome;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt;

#[cfg(test)]
mod tests;

const FORMAT_VERSION: u32 = 1;

fn default_enabled() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectConfig {
    pub format_version: u32,
    #[serde(default)]
    pub locking: LockConfig,
    pub statuses: Vec<StatusDefinition>,
    #[serde(default)]
    pub transitions: Vec<StatusTransition>,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            format_version: FORMAT_VERSION,
            locking: LockConfig::default(),
            statuses: vec![
                StatusDefinition::new("todo", "To Do"),
                StatusDefinition::new("in_progress", "In Progress"),
                StatusDefinition::with_outcome("done", "Done", TaskOutcome::Completed),
            ],
            transitions: vec![
                StatusTransition::new("todo", "in_progress"),
                StatusTransition::new("in_progress", "done"),
            ],
        }
    }
}

impl ProjectConfig {
    pub fn from_toml(source: &str) -> Result<Self, ConfigError> {
        let config: Self = toml::from_str(source)
            .map_err(|error| ConfigError::Parse(error.to_string()))?;
        config.validate()?;
        Ok(config)
    }

    pub fn to_toml(&self) -> Result<String, ConfigError> {
        self.validate()?;
        toml::to_string_pretty(self).map_err(|error| ConfigError::Serialize(error.to_string()))
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.format_version != FORMAT_VERSION {
            return Err(ConfigError::UnsupportedFormatVersion(self.format_version));
        }

        if self.statuses.is_empty() {
            return Err(ConfigError::NoStatuses);
        }

        if self.locking.lease_seconds < 60
            || self.locking.heartbeat_seconds == 0
            || (self.locking.heartbeat_seconds as u128) * 3
                >= self.locking.lease_seconds as u128
        {
            return Err(ConfigError::InvalidLockSettings(
                "lease must be at least 60 seconds and heartbeat must be positive and less than one third of the lease".into(),
            ));
        }

        let mut statuses = HashMap::new();
        for status in &self.statuses {
            if crate::domain::StatusId::new(status.id.clone()).is_err() {
                return Err(ConfigError::InvalidStatusId(status.id.clone()));
            }
            if status.label.trim().is_empty() {
                return Err(ConfigError::EmptyStatusLabel(status.id.clone()));
            }
            if statuses.insert(status.id.as_str(), status).is_some() {
                return Err(ConfigError::DuplicateStatusId(status.id.clone()));
            }
        }

        if !self.statuses.iter().any(|status| status.enabled) {
            return Err(ConfigError::NoEnabledStatuses);
        }

        let mut transitions = HashSet::new();
        for transition in &self.transitions {
            let Some(source) = statuses.get(transition.from.as_str()) else {
                return Err(ConfigError::UnknownTransitionEndpoint {
                    from: transition.from.clone(),
                    to: transition.to.clone(),
                });
            };
            let Some(target) = statuses.get(transition.to.as_str()) else {
                return Err(ConfigError::UnknownTransitionEndpoint {
                    from: transition.from.clone(),
                    to: transition.to.clone(),
                });
            };
            if !target.enabled {
                return Err(ConfigError::DisabledTransitionTarget(target.id.clone()));
            }
            if transition.from == transition.to {
                return Err(ConfigError::SelfTransition(transition.from.clone()));
            }
            if !transitions.insert((transition.from.as_str(), transition.to.as_str())) {
                return Err(ConfigError::DuplicateTransition {
                    from: transition.from.clone(),
                    to: transition.to.clone(),
                });
            }
            let _ = source;
        }

        Ok(())
    }

    pub fn is_transition_allowed(&self, from: &str, to: &str) -> bool {
        self.statuses.iter().any(|status| status.id == from)
            && self.statuses.iter().any(|status| status.id == to && status.enabled)
            && self.transitions.iter().any(|transition| {
                transition.from == from && transition.to == to
            })
    }

    pub fn outcome_for_status(&self, status_id: &str) -> Option<TaskOutcome> {
        self.statuses
            .iter()
            .find(|status| status.id == status_id)
            .and_then(|status| status.outcome)
    }

    pub fn first_enabled_status(&self) -> Option<&StatusDefinition> {
        self.statuses.iter().find(|status| status.enabled)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockConfig {
    #[serde(default = "default_lease_seconds")]
    pub lease_seconds: u64,
    #[serde(default = "default_heartbeat_seconds")]
    pub heartbeat_seconds: u64,
    #[serde(default = "default_recovery_grace_seconds")]
    pub recovery_grace_seconds: u64,
}

impl Default for LockConfig {
    fn default() -> Self {
        Self {
            lease_seconds: default_lease_seconds(),
            heartbeat_seconds: default_heartbeat_seconds(),
            recovery_grace_seconds: default_recovery_grace_seconds(),
        }
    }
}

fn default_lease_seconds() -> u64 { 300 }
fn default_heartbeat_seconds() -> u64 { 60 }
fn default_recovery_grace_seconds() -> u64 { 60 }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatusDefinition {
    pub id: String,
    pub label: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub outcome: Option<TaskOutcome>,
}

impl StatusDefinition {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            enabled: true,
            outcome: None,
        }
    }

    pub fn with_outcome(
        id: impl Into<String>,
        label: impl Into<String>,
        outcome: TaskOutcome,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            enabled: true,
            outcome: Some(outcome),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatusTransition {
    pub from: String,
    pub to: String,
}

impl StatusTransition {
    pub fn new(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigError {
    Parse(String),
    Serialize(String),
    UnsupportedFormatVersion(u32),
    NoStatuses,
    NoEnabledStatuses,
    DuplicateStatusId(String),
    InvalidStatusId(String),
    EmptyStatusLabel(String),
    UnknownTransitionEndpoint { from: String, to: String },
    DisabledTransitionTarget(String),
    SelfTransition(String),
    DuplicateTransition { from: String, to: String },
    InvalidLockSettings(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(message) => write!(formatter, "Invalid TOML configuration: {message}"),
            Self::Serialize(message) => write!(formatter, "Cannot serialize configuration: {message}"),
            Self::UnsupportedFormatVersion(version) => write!(formatter, "Unsupported config format version: {version}"),
            Self::NoStatuses => formatter.write_str("At least one status must be configured"),
            Self::NoEnabledStatuses => formatter.write_str("At least one status must be enabled"),
            Self::DuplicateStatusId(id) => write!(formatter, "Duplicate status ID: {id}"),
            Self::InvalidStatusId(id) => write!(formatter, "Invalid status ID: {id}"),
            Self::EmptyStatusLabel(id) => write!(formatter, "Status label cannot be empty: {id}"),
            Self::UnknownTransitionEndpoint { from, to } => write!(formatter, "Unknown transition endpoint: {from} -> {to}"),
            Self::DisabledTransitionTarget(id) => write!(formatter, "Transition target is disabled: {id}"),
            Self::SelfTransition(id) => write!(formatter, "Self-transitions are not allowed: {id}"),
            Self::DuplicateTransition { from, to } => write!(formatter, "Duplicate transition: {from} -> {to}"),
            Self::InvalidLockSettings(message) => write!(formatter, "Invalid lock settings: {message}"),
        }
    }
}

impl std::error::Error for ConfigError {}
