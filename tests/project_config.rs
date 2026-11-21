use tempfile::tempdir;
use todoing::config::{ProjectConfig, StatusOutcome};

#[test]
fn creates_default_toml_config_without_overwriting_existing_config() {
    let dir = tempdir().unwrap();
    let config = ProjectConfig::load_or_create(dir.path()).unwrap();

    assert_eq!(config.schema_version, 1);
    assert_eq!(config.default_status, "todo");
    assert_eq!(config.statuses.len(), 4);
    assert_eq!(config.outcome_for("done"), Some(StatusOutcome::Completed));
    assert_eq!(
        config.outcome_for("cancelled"),
        Some(StatusOutcome::Cancelled)
    );

    let path = ProjectConfig::config_path(dir.path());
    let original = std::fs::read_to_string(&path).unwrap();
    let loaded = ProjectConfig::load_or_create(dir.path()).unwrap();
    assert_eq!(loaded, config);
    assert_eq!(std::fs::read_to_string(path).unwrap(), original);
}

#[test]
fn default_workflow_allows_only_explicit_transitions() {
    let config = ProjectConfig::default();

    assert!(config.validate_transition("todo", "in_progress").is_ok());
    assert!(config.validate_transition("in_progress", "done").is_ok());
    assert!(config.validate_transition("done", "in_progress").is_ok());
    assert!(config.validate_transition("todo", "done").is_err());
    assert!(
        config
            .validate_transition("in_progress", "in_progress")
            .is_err()
    );
}

#[test]
fn rejects_duplicate_status_ids_and_missing_transition_endpoints() {
    let mut config = ProjectConfig::default();
    config.statuses.push(config.statuses[0].clone());
    assert!(config.validate().is_err());

    let mut config = ProjectConfig::default();
    config.transitions[0].to = "missing".into();
    assert!(config.validate().is_err());
}

#[test]
fn disabled_status_cannot_be_a_target_but_can_be_a_source() {
    let mut config = ProjectConfig::default();
    config
        .statuses
        .iter_mut()
        .find(|status| status.id == "in_progress")
        .unwrap()
        .enabled = false;

    assert!(config.validate().is_err());
    config
        .transitions
        .retain(|transition| transition.to != "in_progress");
    config.validate().unwrap();
    assert!(config.validate_new_status("in_progress").is_err());
    assert!(config.validate_transition("in_progress", "todo").is_ok());
}

#[test]
fn default_status_must_be_enabled_and_have_no_terminal_outcome() {
    let mut config = ProjectConfig::default();
    config.default_status = "done".into();
    assert!(config.validate().is_err());

    let mut config = ProjectConfig::default();
    config
        .statuses
        .iter_mut()
        .find(|status| status.id == "todo")
        .unwrap()
        .enabled = false;
    assert!(config.validate().is_err());
}

#[test]
fn rejects_unsupported_configuration_schema_version() {
    let mut config = ProjectConfig::default();
    config.schema_version = 99;
    assert!(config.validate().is_err());
}

#[test]
fn existing_invalid_configuration_is_not_replaced_by_defaults() {
    let dir = tempdir().unwrap();
    let path = ProjectConfig::config_path(dir.path());
    std::fs::create_dir_all(dir.path()).unwrap();
    std::fs::write(&path, "schema_version = 999\ndefault_status = \"todo\"\n").unwrap();

    assert!(ProjectConfig::load_or_create(dir.path()).is_err());
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        "schema_version = 999\ndefault_status = \"todo\"\n"
    );
}

#[test]
fn rejects_unknown_configuration_fields() {
    let dir = tempdir().unwrap();
    let path = ProjectConfig::config_path(dir.path());
    std::fs::create_dir_all(dir.path()).unwrap();

    let mut content = toml::to_string_pretty(&ProjectConfig::default()).unwrap();
    content.insert_str(0, "unknown_option = true\n");
    std::fs::write(&path, &content).unwrap();

    assert!(ProjectConfig::load_or_create(dir.path()).is_err());
    assert_eq!(std::fs::read_to_string(path).unwrap(), content);
}

#[test]
fn concurrent_first_open_creates_one_complete_default_config() {
    use std::sync::{Arc, Barrier};
    use std::thread;

    let dir = tempdir().unwrap();
    let project_dir = dir.path().to_path_buf();
    let barrier = Arc::new(Barrier::new(8));
    let workers = (0..8)
        .map(|_| {
            let project_dir = project_dir.clone();
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                ProjectConfig::load_or_create(project_dir).unwrap()
            })
        })
        .collect::<Vec<_>>();

    for worker in workers {
        assert_eq!(worker.join().unwrap(), ProjectConfig::default());
    }

    let path = ProjectConfig::config_path(dir.path());
    let content = std::fs::read_to_string(path).unwrap();
    let loaded: ProjectConfig = toml::from_str(&content).unwrap();
    assert_eq!(loaded, ProjectConfig::default());
}
