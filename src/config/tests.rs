use super::{ConfigError, ProjectConfig, StatusDefinition, StatusTransition};
use crate::domain::TaskOutcome;

fn sample() -> ProjectConfig {
    ProjectConfig {
        format_version: 1,
        locking: Default::default(),
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

#[test]
fn default_configuration_is_generic_and_valid() {
    let config = ProjectConfig::default();
    config.validate().unwrap();
    assert_eq!(config.statuses[0].id, "todo");
    assert_eq!(config.statuses[2].outcome, Some(TaskOutcome::Completed));
    assert!(config.is_transition_allowed("todo", "in_progress"));
    assert!(!config.is_transition_allowed("todo", "done"));
}

#[test]
fn toml_round_trip_preserves_statuses_transitions_and_lock_settings() {
    let original = sample();
    let raw = original.to_toml().unwrap();
    let parsed = ProjectConfig::from_toml(&raw).unwrap();
    assert_eq!(parsed, original);
}

#[test]
fn rejects_unsupported_format_version() {
    let raw = sample().to_toml().unwrap().replace("format_version = 1", "format_version = 2");
    assert!(matches!(ProjectConfig::from_toml(&raw), Err(ConfigError::UnsupportedFormatVersion(2))));
}

#[test]
fn rejects_duplicate_status_ids() {
    let mut config = sample();
    config.statuses.push(StatusDefinition::new("todo", "Duplicate"));
    assert!(matches!(config.validate(), Err(ConfigError::DuplicateStatusId(_))));
}

#[test]
fn rejects_invalid_status_ids() {
    let mut config = sample();
    config.statuses[0].id = "In Progress".into();
    assert!(matches!(config.validate(), Err(ConfigError::InvalidStatusId(_))));
}

#[test]
fn rejects_empty_status_labels() {
    let mut config = sample();
    config.statuses[0].label = "  ".into();
    assert!(matches!(config.validate(), Err(ConfigError::EmptyStatusLabel(_))));
}

#[test]
fn rejects_unknown_transition_endpoints() {
    let mut config = sample();
    config.transitions.push(StatusTransition::new("missing", "done"));
    assert!(matches!(config.validate(), Err(ConfigError::UnknownTransitionEndpoint { .. })));
}

#[test]
fn rejects_transitions_into_disabled_statuses() {
    let mut config = sample();
    config.statuses[1].enabled = false;
    assert!(matches!(config.validate(), Err(ConfigError::DisabledTransitionTarget(_))));
}

#[test]
fn rejects_duplicate_transitions() {
    let mut config = sample();
    config.transitions.push(StatusTransition::new("todo", "in_progress"));
    assert!(matches!(config.validate(), Err(ConfigError::DuplicateTransition { .. })));
}

#[test]
fn rejects_configurations_without_enabled_statuses() {
    let mut config = sample();
    for status in &mut config.statuses {
        status.enabled = false;
    }
    assert!(matches!(config.validate(), Err(ConfigError::NoEnabledStatuses)));
}

#[test]
fn validates_lock_timing_relationship() {
    let mut config = sample();
    config.locking.lease_seconds = 120;
    config.locking.heartbeat_seconds = 39;
    assert!(config.validate().is_ok());

    config.locking.heartbeat_seconds = 40;
    assert!(matches!(config.validate(), Err(ConfigError::InvalidLockSettings(_))));
}

#[test]
fn rejects_unknown_toml_fields() {
    let raw = sample().to_toml().unwrap() + "\nunknown = true\n";
    assert!(matches!(ProjectConfig::from_toml(&raw), Err(ConfigError::Parse(_))));
}

#[test]
fn omitted_optional_fields_use_documented_defaults() {
    let raw = r#"
format_version = 1

[[statuses]]
id = "open"
label = "Open"
"#;
    let config = ProjectConfig::from_toml(raw).unwrap();
    assert!(config.statuses[0].enabled);
    assert_eq!(config.statuses[0].outcome, None);
    assert!(config.transitions.is_empty());
    assert_eq!(config.locking.lease_seconds, 300);
    assert_eq!(config.locking.heartbeat_seconds, 60);
    assert_eq!(config.locking.recovery_grace_seconds, 60);
}
