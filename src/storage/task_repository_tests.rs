use crate::config::ProjectConfig;
use super::{NewTask, Store, TaskRepository};
use crate::domain::{ActorContext, ActorId, ProjectId, StatusId, TaskId, TaskOutcome};
use tempfile::tempdir;

fn actor() -> ActorContext {
    ActorContext::new(ActorId::new("agent:repository-tests").unwrap())
}

fn repository() -> (tempfile::TempDir, TaskRepository) {
    let root = tempdir().unwrap();
    let store = Store::initialize(root.path(), ProjectId::new("task-repository-tests").unwrap()).unwrap();
    let repository = TaskRepository::new(store, ProjectConfig::default()).unwrap();
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

    let first = repository.create(new_task("first"), &actor()).unwrap();
    let second = repository.create(new_task("second"), &actor()).unwrap();

    assert_eq!(first.id.get(), 1);
    assert_eq!(second.id.get(), 2);
    assert_eq!(repository.get(first.id).unwrap(), first);
    assert_eq!(repository.list().unwrap(), vec![first, second]);
}

#[test]
fn update_replaces_mutable_fields_without_changing_task_identity() {
    let (_root, repository) = repository();
    let mut task = repository.create(new_task("before"), &actor()).unwrap();
    let original_id = task.id;

    task.title = "after".into();
    task.status_id = StatusId::new("in_progress").unwrap();
    task.body = "updated body".into();
    task.labels = vec!["reviewed".into()];
    task.commit_refs.clear();

    let updated = repository.update(&task, &actor()).unwrap();

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

    assert!(repository.create(invalid, &actor()).is_err());
    assert!(repository.list().unwrap().is_empty());
}

#[test]
fn blank_titles_and_duplicate_normalized_labels_are_rejected() {
    let (_root, repository) = repository();
    let mut blank_title = new_task("valid");
    blank_title.title = "  ".into();
    assert!(repository.create(blank_title, &actor()).is_err());

    let mut duplicate_labels = new_task("valid");
    duplicate_labels.labels = vec!["Rust".into(), " rust ".into()];
    assert!(repository.create(duplicate_labels, &actor()).is_err());

    assert!(repository.list().unwrap().is_empty());
}

#[test]
fn update_of_missing_task_returns_task_not_found() {
    let (_root, repository) = repository();
    let missing = TaskId::new(42).unwrap();
    let mut task = repository.create(new_task("existing"), &actor()).unwrap();
    task.id = missing;

    assert!(repository.update(&task, &actor()).is_err());
    assert_eq!(repository.list().unwrap().len(), 1);
}

#[test]
fn list_returns_tasks_in_stable_id_order() {
    let (_root, repository) = repository();

    let first = repository.create(new_task("first"), &actor()).unwrap();
    let second = repository.create(new_task("second"), &actor()).unwrap();
    let third = repository.create(new_task("third"), &actor()).unwrap();

    assert_eq!(
        repository.list().unwrap().iter().map(|task| task.id).collect::<Vec<_>>(),
        vec![first.id, second.id, third.id]
    );
}

#[test]
fn terminal_outcome_is_persisted_and_restored() {
    let (_root, repository) = repository();
    let created = repository.create(new_task("finished"), &actor()).unwrap();
    let completed = repository.complete(created.id, &actor()).unwrap();
    let loaded = repository.get(created.id).unwrap();

    assert_eq!(completed.outcome, Some(TaskOutcome::Completed));
    assert_eq!(loaded.outcome, Some(TaskOutcome::Completed));
}


#[test]
fn archive_and_restore_preserve_identity_outcome_and_relationships() {
    let (_root, repository) = repository();
    let prerequisite = repository.create(new_task("prerequisite"), &actor()).unwrap();
    let dependent = repository.create(new_task("dependent"), &actor()).unwrap();
    repository.add_dependency(dependent.id, prerequisite.id, &actor()).unwrap();

    let archived = repository.archive(prerequisite.id, &actor()).unwrap();
    assert!(!archived.no_op);
    assert!(archived.task.archived_at.is_some());
    assert_eq!(archived.task.archived_by.as_ref().unwrap().as_str(), "agent:repository-tests");
    assert_eq!(archived.task.id, prerequisite.id);
    assert_eq!(archived.task.status_id, prerequisite.status_id);
    assert_eq!(archived.task.outcome, prerequisite.outcome);
    assert!(repository.list().unwrap().iter().all(|task| task.id != prerequisite.id));
    assert_eq!(repository.list_archived().unwrap(), vec![archived.task.clone()]);
    assert_eq!(repository.list_all().unwrap().len(), 2);
    assert_eq!(repository.get(prerequisite.id).unwrap(), archived.task);

    assert_eq!(repository.list_dependencies(dependent.id).unwrap(), vec![prerequisite.id]);
    let versions = repository.list_versions(prerequisite.id).unwrap();
    assert_eq!(versions.len(), 2);
    let archived_snapshot: serde_json::Value =
        serde_json::from_str(&versions.last().unwrap().snapshot_json).unwrap();
    assert!(archived_snapshot["archived_at"].is_string());
    assert_eq!(archived_snapshot["archived_by"], "agent:repository-tests");

    let restored = repository.restore(prerequisite.id, &actor()).unwrap();
    assert!(!restored.no_op);
    assert_eq!(restored.task.archived_at, None);
    assert_eq!(restored.task.archived_by, None);
    assert_eq!(restored.task.status_id, prerequisite.status_id);
    assert_eq!(restored.task.outcome, prerequisite.outcome);
    assert_eq!(repository.list().unwrap().len(), 2);
    assert!(repository.list_archived().unwrap().is_empty());
    assert_eq!(repository.list_dependencies(dependent.id).unwrap(), vec![prerequisite.id]);

    let history = repository.get_history(prerequisite.id).unwrap();
    assert!(history.iter().any(|event| event.operation == "task.archive"));
    assert!(history.iter().any(|event| event.operation == "task.restore"));
}

#[test]
fn archive_and_restore_are_idempotent_without_redundant_versions() {
    let (_root, repository) = repository();
    let task = repository.create(new_task("idempotent"), &actor()).unwrap();

    let first_archive = repository.archive(task.id, &actor()).unwrap();
    assert!(!first_archive.no_op);
    let versions_after_archive = repository.list_versions(task.id).unwrap().len();

    let second_archive = repository.archive(task.id, &actor()).unwrap();
    assert!(second_archive.no_op);
    assert_eq!(repository.list_versions(task.id).unwrap().len(), versions_after_archive);

    let first_restore = repository.restore(task.id, &actor()).unwrap();
    assert!(!first_restore.no_op);
    let versions_after_restore = repository.list_versions(task.id).unwrap().len();

    let second_restore = repository.restore(task.id, &actor()).unwrap();
    assert!(second_restore.no_op);
    assert_eq!(repository.list_versions(task.id).unwrap().len(), versions_after_restore);

    let history = repository.get_history(task.id).unwrap();
    assert_eq!(history.iter().filter(|event| event.operation == "task.archive").count(), 2);
    assert_eq!(history.iter().filter(|event| event.operation == "task.restore").count(), 2);
}

#[test]
fn archived_tasks_are_read_only_until_restored() {
    let (_root, repository) = repository();
    let task = repository.create(new_task("read only"), &actor()).unwrap();
    let archived = repository.archive(task.id, &actor()).unwrap().task;

    let mut edited = archived.clone();
    edited.title = "must not change".into();
    assert!(matches!(repository.update(&edited, &actor()), Err(super::StoreError::TaskArchived)));

    assert!(matches!(
        repository.add_dependency(archived.id, TaskId::new(999).unwrap(), &actor()),
        Err(super::StoreError::TaskArchived)
    ));
    assert_eq!(repository.get(task.id).unwrap(), archived);

    repository.restore(task.id, &actor()).unwrap();
    let mut edited = repository.get(task.id).unwrap();
    edited.title = "restored and editable".into();
    assert_eq!(repository.update(&edited, &actor()).unwrap().title, "restored and editable");
}

#[test]
fn archive_obeys_existing_task_lock_ownership() {
    let (_root, repository) = repository();
    let task = repository.create(new_task("locked"), &actor()).unwrap();
    let service = repository.lock_service().unwrap();
    let owner = actor();
    let grant = service.acquire(task.id, "repository-test-owner", &owner).unwrap();
    let other = ActorContext::new(ActorId::new("agent:other").unwrap());

    assert!(matches!(repository.archive(task.id, &other), Err(super::StoreError::TaskLocked)));
    let owner_repository = repository.with_lock_grant(&grant);
    let archived = owner_repository.archive(task.id, &owner).unwrap();
    assert!(archived.task.archived_at.is_some());

    service.release(&grant, &owner).unwrap();
    assert_eq!(repository.list_archived().unwrap().len(), 1);
}


#[test]
fn archive_transaction_rolls_back_task_version_and_history_on_storage_failure() {
    let (_root, repository) = repository();
    let task = repository.create(new_task("rollback"), &actor()).unwrap();
    let store = Store::open(_root.path(), None).unwrap();
    let connection = store.connection().unwrap();
    connection.execute_batch(
        "CREATE TRIGGER reject_task_archive
         BEFORE UPDATE OF archived_at ON tasks
         WHEN NEW.archived_at IS NOT NULL
         BEGIN
             SELECT RAISE(ABORT, 'injected archive failure');
         END;"
    ).unwrap();
    drop(connection);

    assert!(repository.archive(task.id, &actor()).is_err());
    let current = repository.get(task.id).unwrap();
    assert!(current.archived_at.is_none());
    assert!(current.archived_by.is_none());
    assert_eq!(repository.list_versions(task.id).unwrap().len(), 1);
    assert!(repository.get_history(task.id).unwrap().iter()
        .all(|event| event.operation != "task.archive"));
}

#[test]
fn concurrent_archive_requests_are_serialized_and_versioned_once() {
    use std::sync::{Arc, Barrier};
    use std::thread;

    let (_root, repository) = repository();
    let task = repository.create(new_task("concurrent archive"), &actor()).unwrap();
    let barrier = Arc::new(Barrier::new(3));
    let first_repository = repository.clone();
    let second_repository = repository.clone();
    let first_barrier = barrier.clone();
    let second_barrier = barrier.clone();

    let first = thread::spawn(move || {
        first_barrier.wait();
        first_repository.archive(task.id, &actor()).unwrap()
    });
    let second = thread::spawn(move || {
        second_barrier.wait();
        second_repository.archive(task.id, &actor()).unwrap()
    });
    barrier.wait();

    let first_result = first.join().unwrap();
    let second_result = second.join().unwrap();
    assert_ne!(first_result.no_op, second_result.no_op);
    assert!(repository.get(task.id).unwrap().archived_at.is_some());
    assert_eq!(repository.list_versions(task.id).unwrap().len(), 2);
    assert_eq!(repository.get_history(task.id).unwrap().iter()
        .filter(|event| event.operation == "task.archive").count(), 2);
}
