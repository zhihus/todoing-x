use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;

const CONFIG_FILE: &str = "config.toml";
const SUPPORTED_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StatusOutcome {
    Completed,
    Cancelled,
}

impl StatusOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StatusDefinition {
    pub id: String,
    pub label: String,
    pub order: i32,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<StatusOutcome>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StatusTransition {
    pub from: String,
    pub to: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProjectConfig {
    pub schema_version: u32,
    pub default_status: String,
    pub statuses: Vec<StatusDefinition>,
    pub transitions: Vec<StatusTransition>,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            schema_version: SUPPORTED_SCHEMA_VERSION,
            default_status: "todo".into(),
            statuses: vec![
                StatusDefinition {
                    id: "todo".into(),
                    label: "To do".into(),
                    order: 10,
                    enabled: true,
                    outcome: None,
                },
                StatusDefinition {
                    id: "in_progress".into(),
                    label: "In progress".into(),
                    order: 20,
                    enabled: true,
                    outcome: None,
                },
                StatusDefinition {
                    id: "done".into(),
                    label: "Done".into(),
                    order: 30,
                    enabled: true,
                    outcome: Some(StatusOutcome::Completed),
                },
                StatusDefinition {
                    id: "cancelled".into(),
                    label: "Cancelled".into(),
                    order: 40,
                    enabled: true,
                    outcome: Some(StatusOutcome::Cancelled),
                },
            ],
            transitions: vec![
                transition("todo", "in_progress"),
                transition("todo", "cancelled"),
                transition("in_progress", "todo"),
                transition("in_progress", "done"),
                transition("in_progress", "cancelled"),
                transition("done", "in_progress"),
                transition("cancelled", "todo"),
            ],
        }
    }
}

fn transition(from: &str, to: &str) -> StatusTransition {
    StatusTransition {
        from: from.into(),
        to: to.into(),
    }
}

impl ProjectConfig {
    pub fn config_path(todoing_dir: impl AsRef<Path>) -> PathBuf {
        todoing_dir.as_ref().join(CONFIG_FILE)
    }

    pub fn load_or_create(todoing_dir: impl AsRef<Path>) -> Result<Self, String> {
        let todoing_dir = todoing_dir.as_ref();
        fs::create_dir_all(todoing_dir).map_err(|error| {
            format!(
                "Cannot create Todoing configuration directory {}: {error}",
                todoing_dir.display()
            )
        })?;
        let path = Self::config_path(todoing_dir);

        if !path.exists() {
            let default_config = Self::default();
            let serialized = toml::to_string_pretty(&default_config)
                .map_err(|error| format!("Cannot serialize default configuration: {error}"))?;
            let mut temporary = NamedTempFile::new_in(todoing_dir).map_err(|error| {
                format!("Cannot create temporary configuration file: {error}")
            })?;
            temporary.write_all(serialized.as_bytes()).map_err(|error| {
                format!(
                    "Cannot write temporary configuration for {}: {error}",
                    path.display()
                )
            })?;
            temporary.as_file().sync_all().map_err(|error| {
                format!(
                    "Cannot sync temporary configuration for {}: {error}",
                    path.display()
                )
            })?;

            match temporary.persist_noclobber(&path) {
                Ok(_) => {}
                Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => {
                    return Err(format!(
                        "Cannot install default configuration {}: {}",
                        path.display(),
                        error.error
                    ));
                }
            }
        }

        let content = fs::read_to_string(&path).map_err(|error| {
            format!("Cannot read configuration file {}: {error}", path.display())
        })?;
        let config: Self = toml::from_str(&content)
            .map_err(|error| format!("Invalid TOML configuration {}: {error}", path.display()))?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != SUPPORTED_SCHEMA_VERSION {
            return Err(format!(
                "Unsupported configuration schema version {}; supported version is {}",
                self.schema_version, SUPPORTED_SCHEMA_VERSION
            ));
        }
        if self.statuses.is_empty() {
            return Err("At least one status must be configured".into());
        }

        let mut status_ids = HashSet::new();
        for status in &self.statuses {
            if !is_valid_status_id(&status.id) {
                return Err(format!("Invalid status ID '{}'", status.id));
            }
            if status.label.trim().is_empty() {
                return Err(format!("Status '{}' has an empty label", status.id));
            }
            if !status_ids.insert(status.id.as_str()) {
                return Err(format!("Duplicate status ID '{}'", status.id));
            }
        }

        let default = self
            .status(&self.default_status)
            .ok_or_else(|| format!("Default status '{}' is not defined", self.default_status))?;
        if !default.enabled {
            return Err(format!(
                "Default status '{}' must be enabled",
                self.default_status
            ));
        }
        if default.outcome.is_some() {
            return Err("Default status cannot have a terminal outcome".into());
        }

        let mut transitions = HashSet::new();
        for transition in &self.transitions {
            if self.status(&transition.from).is_none() {
                return Err(format!(
                    "Transition source status '{}' is not defined",
                    transition.from
                ));
            }
            let target = self.status(&transition.to).ok_or_else(|| {
                format!(
                    "Transition target status '{}' is not defined",
                    transition.to
                )
            })?;
            if transition.from == transition.to {
                return Err(format!(
                    "Self-transition for status '{}' is not allowed",
                    transition.from
                ));
            }
            if !target.enabled {
                return Err(format!(
                    "Disabled status '{}' cannot be a transition target",
                    transition.to
                ));
            }
            if !transitions.insert((transition.from.as_str(), transition.to.as_str())) {
                return Err(format!(
                    "Duplicate transition '{}' -> '{}'",
                    transition.from, transition.to
                ));
            }
        }

        Ok(())
    }

    pub fn status(&self, id: &str) -> Option<&StatusDefinition> {
        self.statuses.iter().find(|status| status.id == id)
    }

    pub fn validate_new_status(&self, status_id: &str) -> Result<(), String> {
        let status = self
            .status(status_id)
            .ok_or_else(|| format!("Status '{status_id}' is not configured"))?;
        if !status.enabled {
            return Err(format!("Status '{status_id}' is disabled"));
        }
        Ok(())
    }

    pub fn validate_transition(&self, from: &str, to: &str) -> Result<(), String> {
        let source = self
            .status(from)
            .ok_or_else(|| format!("Current status '{from}' is not configured"))?;
        let target = self
            .status(to)
            .ok_or_else(|| format!("Target status '{to}' is not configured"))?;
        if !target.enabled {
            return Err(format!("Target status '{to}' is disabled"));
        }
        if source.id == target.id {
            return Err("A task cannot transition to its current status".into());
        }
        if !self
            .transitions
            .iter()
            .any(|transition| transition.from == from && transition.to == to)
        {
            return Err(format!("Transition '{from}' -> '{to}' is not permitted"));
        }
        Ok(())
    }

    pub fn outcome_for(&self, status_id: &str) -> Option<StatusOutcome> {
        self.status(status_id).and_then(|status| status.outcome)
    }
}

fn is_valid_status_id(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_' || byte == b'-'
        })
}
