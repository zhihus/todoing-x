use super::{NewTask, Store, StoreError, TaskRepository};
use crate::{
    config::{ProjectConfig, StatusTransition},
    domain::{ActorContext, ActorId, ProjectId, StatusId, TaskOutcome},
};
use tempfile::tempdir;

fn actor() -> ActorContext {
    ActorContext::new(ActorId::new("agent:status-tests").unwrap())
}

fn repository(config: ProjectConfig) -> (tempfile::TempDir, TaskRepository) {
    let root = tempdir().unwrap();
    let store = Store::initialize(root.path(), ProjectId::new("status-tests").unwrap()).unwrap();
    (root, TaskRepository::new(store, config).unwrap())
}

fn new_task(title: &str) -> NewTask {
    NewTask {
        title: title.into(),
        status_id: StatusId::new("todo").unwrap(),
        outcome: None,
        body: String::new(),
        labels: vec![],
        commit_refs: vec![],
    }
}

#[test]
fn configured_transition_changes_status_and_records_config_revision() {
    let (_root, repository) = repository(ProjectConfig::default());
    let task = repository.create(new_task("workflow"), &actor()).unwrap();

    let updated = repository
        .set_status(task.id, StatusId::new("in_progress").unwrap(), &actor())
        .unwrap();

    assert_eq!(updated.status_id.as_str(), "in_progress");
    assert_eq!(updated.outcome, None);
    let history = repository.get_history(task.id).unwrap();
    assert_eq!(history[1].operation, "task.status.set");
    assert!(history[1].config_revision.is_some());
}

#[test]
fn forbidden_transition_is_rejected_and_recorded_without_mutation() {
    let (_root, repository) = repository(ProjectConfig::default());
    let task = repository.create(new_task("workflow"), &actor()).unwrap();

    assert!(matches!(
        repository.set_status(task.id, StatusId::new("done").unwrap(), &actor()),
        Err(StoreError::InvalidStatusTransition(_))
    ));

    assert_eq!(repository.get(task.id).unwrap(), task);
    assert_eq!(repository.list_versions(task.id).unwrap().len(), 1);
    let history = repository.get_history(task.id).unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[1].outcome, super::HistoryOutcome::Rejected);
}

#[test]
fn entering_a_status_with_completed_outcome_sets_task_outcome() {
    let (_root, repository) = repository(ProjectConfig::default());
    let task = repository.create(new_task("workflow"), &actor()).unwrap();
    let in_progress = repository
        .set_status(task.id, StatusId::new("in_progress").unwrap(), &actor())
        .unwrap();

    let completed = repository
        .set_status(in_progress.id, StatusId::new("done").unwrap(), &actor())
        .unwrap();

    assert_eq!(completed.status_id.as_str(), "done");
    assert_eq!(completed.outcome, Some(TaskOutcome::Completed));
}

#[test]
fn terminal_task_cannot_use_normal_status_transition_or_update() {
    let (_root, repository) = repository(ProjectConfig::default());
    let task = repository.create(new_task("workflow"), &actor()).unwrap();
    repository.complete(task.id, &actor()).unwrap();

    assert!(matches!(
        repository.set_status(task.id, StatusId::new("in_progress").unwrap(), &actor()),
        Err(StoreError::TaskTerminal)
    ));

    let mut edited = repository.get(task.id).unwrap();
    edited.title = "attempted edit".into();
    assert!(matches!(
        repository.update(&edited, &actor()),
        Err(StoreError::TaskTerminal)
    ));
}

#[test]
fn complete_cancel_and_reopen_preserve_distinct_outcomes_and_reason() {
    let (_root, repository) = repository(ProjectConfig::default());
    let task = repository.create(new_task("outcomes"), &actor()).unwrap();

    let completed = repository.complete(task.id, &actor()).unwrap();
    assert_eq!(completed.status_id.as_str(), "todo");
    assert_eq!(completed.outcome, Some(TaskOutcome::Completed));

    let reopened = repository.reopen(task.id, None, &actor()).unwrap();
    assert_eq!(reopened.outcome, None);

    let cancelled = repository
        .cancel(task.id, Some("No longer required"), &actor())
        .unwrap();
    assert_eq!(cancelled.outcome, Some(TaskOutcome::Cancelled));

    let history = repository.get_history(task.id).unwrap();
    assert!(history.iter().any(|entry| {
        entry.operation == "task.cancel"
            && entry.details_json.as_deref().unwrap_or("").contains("No longer required")
    }));
}

#[test]
fn reopening_a_terminal_status_requires_a_valid_nonterminal_target() {
    let mut config = ProjectConfig::default();
    config.transitions.push(StatusTransition::new("done", "in_progress"));
    let (_root, repository) = repository(config);
    let task = repository.create(new_task("reopen mapped status"), &actor()).unwrap();
    let active = repository
        .set_status(task.id, StatusId::new("in_progress").unwrap(), &actor())
        .unwrap();
    let done = repository
        .set_status(active.id, StatusId::new("done").unwrap(), &actor())
        .unwrap();

    assert!(matches!(
        repository.reopen(done.id, None, &actor()),
        Err(StoreError::InvalidStatusTransition(_))
    ));

    let reopened = repository
        .reopen(done.id, Some(StatusId::new("in_progress").unwrap()), &actor())
        .unwrap();
    assert_eq!(reopened.status_id.as_str(), "in_progress");
    assert_eq!(reopened.outcome, None);
}

#[test]
fn disabled_status_cannot_be_assigned_to_a_task() {
    let mut config = ProjectConfig::default();
    config.statuses.push(crate::config::StatusDefinition {
        id: "paused".into(),
        label: "Paused".into(),
        enabled: false,
        outcome: None,
    });
    let (_root, repository) = repository(config);
    let task = repository.create(new_task("disabled"), &actor()).unwrap();

    assert!(matches!(
        repository.set_status(task.id, StatusId::new("paused").unwrap(), &actor()),
        Err(StoreError::InvalidStatusTransition(_))
    ));
}
