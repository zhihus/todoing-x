use super::{NewTask, Store, TaskRepository};
use crate::domain::{ProjectId, StatusId, TaskId, TaskOutcome};
use tempfile::tempdir;

fn repository() -> (tempfile::TempDir, TaskRepository) {
    let root = tempdir().unwrap();
    let store = Store::initialize(root.path(), ProjectId::new("task-repository-tests").unwrap()).unwrap();
    let repository = TaskRepository::new(store);
    (root, repository)
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
fn create_persists_task_and_allocates_stable_monotonic_ids() {
    let (_root, repository) = repository();

    let first = repository.create(new_task("first")).unwrap();
    let second = repository.create(new_task("second")).unwrap();

    assert_eq!(first.id.get(), 1);
    assert_eq!(second.id.get(), 2);
    assert_eq!(repository.get(first.id).unwrap(), first);
    assert_eq!(repository.list().unwrap(), vec![first, second]);
}

#[test]
fn update_replaces_mutable_fields_without_changing_task_identity() {
    let (_root, repository) = repository();
    let mut task = repository.create(new_task("before")).unwrap();
    let original_id = task.id;

    task.title = "after".into();
    task.status_id = StatusId::new("in_progress").unwrap();
    task.body = "updated body".into();
    task.labels = vec!["reviewed".into()];
    task.commit_refs.clear();

    let updated = repository.update(&task).unwrap();

    assert_eq!(updated.id, original_id);
    assert_eq!(updated.title, "after");
    assert_eq!(updated.status_id.as_str(), "in_progress");
    assert_eq!(updated.body, "updated body");
    assert_eq!(updated.labels, vec!["reviewed"]);
    assert!(updated.commit_refs.is_empty());
    assert_eq!(repository.get(original_id).unwrap(), updated);
}

#[test]
fn invalid_task_input_is_rejected_without_partial_persistence() {
    let (_root, repository) = repository();
    let mut invalid = new_task("invalid");
    invalid.commit_refs = vec!["not-a-git-sha".into()];

    assert!(repository.create(invalid).is_err());
    assert!(repository.list().unwrap().is_empty());
}

#[test]
fn blank_titles_and_duplicate_normalized_labels_are_rejected() {
    let (_root, repository) = repository();
    let mut blank_title = new_task("valid");
    blank_title.title = "  ".into();
    assert!(repository.create(blank_title).is_err());

    let mut duplicate_labels = new_task("valid");
    duplicate_labels.labels = vec!["Rust".into(), " rust ".into()];
    assert!(repository.create(duplicate_labels).is_err());

    assert!(repository.list().unwrap().is_empty());
}

#[test]
fn update_of_missing_task_returns_task_not_found() {
    let (_root, repository) = repository();
    let missing = TaskId::new(42).unwrap();
    let mut task = repository.create(new_task("existing")).unwrap();
    task.id = missing;

    assert!(repository.update(&task).is_err());
    assert_eq!(repository.list().unwrap().len(), 1);
}

#[test]
fn list_returns_tasks_in_stable_id_order() {
    let (_root, repository) = repository();

    let first = repository.create(new_task("first")).unwrap();
    let second = repository.create(new_task("second")).unwrap();
    let third = repository.create(new_task("third")).unwrap();

    assert_eq!(
        repository.list().unwrap().iter().map(|task| task.id).collect::<Vec<_>>(),
        vec![first.id, second.id, third.id]
    );
}

#[test]
fn terminal_outcome_is_persisted_and_restored() {
    let (_root, repository) = repository();
    let mut task = new_task("finished");
    task.outcome = Some(TaskOutcome::Completed);

    let created = repository.create(task).unwrap();
    let loaded = repository.get(created.id).unwrap();

    assert_eq!(loaded.outcome, Some(TaskOutcome::Completed));
}
