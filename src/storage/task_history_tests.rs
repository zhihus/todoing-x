use super::{HistoryOutcome, NewTask, Store, TaskRepository};
use crate::domain::{ActorContext, ActorId, ProjectId, StatusId, TaskOutcome};
use tempfile::tempdir;

fn actor() -> ActorContext {
    ActorContext::new(ActorId::new("agent:test").unwrap())
}

fn repository() -> (tempfile::TempDir, TaskRepository) {
    let root = tempdir().unwrap();
    let store = Store::initialize(root.path(), ProjectId::new("task-history-tests").unwrap()).unwrap();
    (root, TaskRepository::new(store))
}

fn new_task(title: &str) -> NewTask {
    NewTask {
        title: title.into(),
        status_id: StatusId::new("todo").unwrap(),
        outcome: None,
        body: format!("body for {title}"),
        labels: vec!["Rust".into(), "MVP".into()],
        commit_refs: vec!["0123456789abcdef0123456789abcdef01234567".into()],
    }
}

#[test]
fn create_writes_initial_immutable_version_and_history_atomically() {
    let (_root, repository) = repository();
    let created = repository.create(new_task("first"), &actor()).unwrap();

    let versions = repository.list_versions(created.id).unwrap();
    let history = repository.get_history(created.id).unwrap();

    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0].version_no, 1);
    assert_eq!(versions[0].actor_id.as_str(), "agent:test");
    assert!(versions[0].snapshot_json.contains("\"title\":\"first\""));
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].operation, "task.create");
    assert_eq!(history[0].outcome, HistoryOutcome::Succeeded);
    assert_eq!(history[0].actor_id.as_str(), "agent:test");
}

#[test]
fn update_writes_next_version_and_preserves_previous_snapshot() {
    let (_root, repository) = repository();
    let mut task = repository.create(new_task("before"), &actor()).unwrap();
    let first_snapshot = repository.get_version(task.id, 1).unwrap().snapshot_json;

    task.title = "after".into();
    task.body = "updated body".into();
    let updated = repository.update(&task, &actor()).unwrap();

    let versions = repository.list_versions(updated.id).unwrap();
    assert_eq!(versions.len(), 2);
    assert_eq!(versions[0].snapshot_json, first_snapshot);
    assert!(versions[1].snapshot_json.contains("\"title\":\"after\""));
    assert_eq!(versions[1].version_no, 2);
    assert_eq!(repository.get_history(updated.id).unwrap().len(), 2);
}

#[test]
fn no_op_update_does_not_create_redundant_version_or_history() {
    let (_root, repository) = repository();
    let task = repository.create(new_task("unchanged"), &actor()).unwrap();

    let updated = repository.update(&task, &actor()).unwrap();

    assert_eq!(updated, task);
    assert_eq!(repository.list_versions(task.id).unwrap().len(), 1);
    assert_eq!(repository.get_history(task.id).unwrap().len(), 1);
}

#[test]
fn baseline_approval_references_a_version_of_the_same_task_and_is_audited() {
    let (_root, repository) = repository();
    let task = repository.create(new_task("baseline"), &actor()).unwrap();

    let baseline = repository.approve_baseline(task.id, 1, &actor()).unwrap();

    assert_eq!(baseline.task_id, task.id);
    assert_eq!(baseline.version_no, 1);
    assert_eq!(baseline.approved_by.as_str(), "agent:test");
    let history = repository.get_history(task.id).unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[1].operation, "task.version.approve");
}

#[test]
fn replacing_baseline_keeps_previous_approval_in_append_only_history() {
    let (_root, repository) = repository();
    let mut task = repository.create(new_task("baseline"), &actor()).unwrap();
    repository.approve_baseline(task.id, 1, &actor()).unwrap();

    task.body = "second version".into();
    repository.update(&task, &actor()).unwrap();
    let baseline = repository.approve_baseline(task.id, 2, &actor()).unwrap();

    assert_eq!(baseline.version_no, 2);
    let approvals = repository.get_history(task.id).unwrap()
        .into_iter()
        .filter(|entry| entry.operation == "task.version.approve")
        .count();
    assert_eq!(approvals, 2);
}

#[test]
fn rejected_create_is_recorded_after_the_failed_operation() {
    let (_root, repository) = repository();
    let mut invalid = new_task("invalid");
    invalid.title = " ".into();

    assert!(repository.create(invalid, &actor()).is_err());
    let history = repository.get_store_history().unwrap();

    assert_eq!(history.len(), 1);
    assert_eq!(history[0].operation, "task.create");
    assert_eq!(history[0].outcome, HistoryOutcome::Rejected);
    assert_eq!(history[0].error_code.as_deref(), Some("invalid_request"));
}

#[test]
fn version_and_history_rows_are_immutable_at_database_level() {
    let (_root, repository) = repository();
    let task = repository.create(new_task("immutable"), &actor()).unwrap();
    let connection = repository.store().connection().unwrap();

    assert!(connection.execute(
        "UPDATE task_versions SET snapshot_json = '{}' WHERE task_id = ?1",
        [task.id.get()]
    ).is_err());
    assert!(connection.execute(
        "DELETE FROM task_versions WHERE task_id = ?1",
        [task.id.get()]
    ).is_err());
    assert!(connection.execute(
        "UPDATE operation_history SET operation = 'changed' WHERE task_id = ?1",
        [task.id.get()]
    ).is_err());
    assert!(connection.execute(
        "DELETE FROM operation_history WHERE task_id = ?1",
        [task.id.get()]
    ).is_err());
}

#[test]
fn snapshots_preserve_terminal_outcomes() {
    let (_root, repository) = repository();
    let mut task = new_task("completed");
    task.outcome = Some(TaskOutcome::Completed);

    let created = repository.create(task, &actor()).unwrap();
    let version = repository.get_version(created.id, 1).unwrap();

    assert!(version.snapshot_json.contains("\"outcome\":\"completed\""));
}
