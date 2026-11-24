use super::{NewTask, Store, TaskRepository};
use crate::config::ProjectConfig;
use crate::domain::{ActorContext, ActorId, ProjectId, StatusId, TaskId, TaskOutcome};
use tempfile::tempdir;

fn actor() -> ActorContext {
    ActorContext::new(ActorId::new("agent:repository-tests").unwrap())
}

fn repository() -> (tempfile::TempDir, TaskRepository) {
    let root = tempdir().unwrap();
    let store = Store::initialize(
        root.path(),
        ProjectId::new("task-repository-tests").unwrap(),
    )
    .unwrap();
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
        repository
            .list()
            .unwrap()
            .iter()
            .map(|task| task.id)
            .collect::<Vec<_>>(),
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
    let prerequisite = repository
        .create(new_task("prerequisite"), &actor())
        .unwrap();
    let dependent = repository.create(new_task("dependent"), &actor()).unwrap();
    repository
        .add_dependency(dependent.id, prerequisite.id, &actor())
        .unwrap();

    let archived = repository.archive(prerequisite.id, &actor()).unwrap();
    assert!(!archived.no_op);
    assert!(archived.task.archived_at.is_some());
    assert_eq!(
        archived.task.archived_by.as_ref().unwrap().as_str(),
        "agent:repository-tests"
    );
    assert_eq!(archived.task.id, prerequisite.id);
    assert_eq!(archived.task.status_id, prerequisite.status_id);
    assert_eq!(archived.task.outcome, prerequisite.outcome);
    assert!(
        repository
            .list()
            .unwrap()
            .iter()
            .all(|task| task.id != prerequisite.id)
    );
    assert_eq!(
        repository.list_archived().unwrap(),
        vec![archived.task.clone()]
    );
    assert_eq!(repository.list_all().unwrap().len(), 2);
    assert_eq!(repository.get(prerequisite.id).unwrap(), archived.task);

    assert_eq!(
        repository.list_dependencies(dependent.id).unwrap(),
        vec![prerequisite.id]
    );
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
    assert_eq!(
        repository.list_dependencies(dependent.id).unwrap(),
        vec![prerequisite.id]
    );

    let history = repository.get_history(prerequisite.id).unwrap();
    assert!(
        history
            .iter()
            .any(|event| event.operation == "task.archive")
    );
    assert!(
        history
            .iter()
            .any(|event| event.operation == "task.restore")
    );
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
    assert_eq!(
        repository.list_versions(task.id).unwrap().len(),
        versions_after_archive
    );

    let first_restore = repository.restore(task.id, &actor()).unwrap();
    assert!(!first_restore.no_op);
    let versions_after_restore = repository.list_versions(task.id).unwrap().len();

    let second_restore = repository.restore(task.id, &actor()).unwrap();
    assert!(second_restore.no_op);
    assert_eq!(
        repository.list_versions(task.id).unwrap().len(),
        versions_after_restore
    );

    let history = repository.get_history(task.id).unwrap();
    assert_eq!(
        history
            .iter()
            .filter(|event| event.operation == "task.archive")
            .count(),
        2
    );
    assert_eq!(
        history
            .iter()
            .filter(|event| event.operation == "task.restore")
            .count(),
        2
    );
}

#[test]
fn archived_tasks_are_read_only_until_restored() {
    let (_root, repository) = repository();
    let task = repository.create(new_task("read only"), &actor()).unwrap();
    let archived = repository.archive(task.id, &actor()).unwrap().task;

    let mut edited = archived.clone();
    edited.title = "must not change".into();
    assert!(matches!(
        repository.update(&edited, &actor()),
        Err(super::StoreError::TaskArchived)
    ));

    assert!(matches!(
        repository.add_dependency(archived.id, TaskId::new(999).unwrap(), &actor()),
        Err(super::StoreError::TaskArchived)
    ));
    assert_eq!(repository.get(task.id).unwrap(), archived);

    repository.restore(task.id, &actor()).unwrap();
    let mut edited = repository.get(task.id).unwrap();
    edited.title = "restored and editable".into();
    assert_eq!(
        repository.update(&edited, &actor()).unwrap().title,
        "restored and editable"
    );
}

#[test]
fn archive_obeys_existing_task_lock_ownership() {
    let (_root, repository) = repository();
    let task = repository.create(new_task("locked"), &actor()).unwrap();
    let service = repository.lock_service().unwrap();
    let owner = actor();
    let grant = service
        .acquire(task.id, "repository-test-owner", &owner)
        .unwrap();
    let other = ActorContext::new(ActorId::new("agent:other").unwrap());

    assert!(matches!(
        repository.archive(task.id, &other),
        Err(super::StoreError::TaskLocked)
    ));
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
    connection
        .execute_batch(
            "CREATE TRIGGER reject_task_archive
         BEFORE UPDATE OF archived_at ON tasks
         WHEN NEW.archived_at IS NOT NULL
         BEGIN
             SELECT RAISE(ABORT, 'injected archive failure');
         END;",
        )
        .unwrap();
    drop(connection);

    assert!(repository.archive(task.id, &actor()).is_err());
    let current = repository.get(task.id).unwrap();
    assert!(current.archived_at.is_none());
    assert!(current.archived_by.is_none());
    assert_eq!(repository.list_versions(task.id).unwrap().len(), 1);
    assert!(
        repository
            .get_history(task.id)
            .unwrap()
            .iter()
            .all(|event| event.operation != "task.archive")
    );
}

#[test]
fn concurrent_archive_requests_are_serialized_and_versioned_once() {
    use std::sync::{Arc, Barrier};
    use std::thread;

    let (_root, repository) = repository();
    let task = repository
        .create(new_task("concurrent archive"), &actor())
        .unwrap();
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
    assert_eq!(
        repository
            .get_history(task.id)
            .unwrap()
            .iter()
            .filter(|event| event.operation == "task.archive")
            .count(),
        2
    );
}

#[test]
fn verified_snapshot_preserves_archived_task_metadata() {
    use rusqlite::Connection;

    let (root, repository) = repository();
    let task = repository
        .create(new_task("snapshot archive"), &actor())
        .unwrap();
    repository.archive(task.id, &actor()).unwrap();

    let store = Store::open(root.path(), None).unwrap();
    let destination = root.path().join("archive-snapshot.db");
    let report = store.snapshot(&destination, false).unwrap();
    assert_eq!(report.path.file_name(), destination.file_name());
    assert!(report.path.is_file());

    let snapshot = Connection::open(&destination).unwrap();
    let archived: (Option<String>, Option<String>) = snapshot
        .query_row(
            "SELECT archived_at, archived_by FROM tasks WHERE task_id = ?1",
            [task.id.get()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert!(archived.0.is_some());
    assert_eq!(archived.1.as_deref(), Some("agent:repository-tests"));
}

#[test]
fn external_references_are_idempotent_removable_and_versioned() {
    let (_root, repository) = repository();
    let task = repository.create(new_task("references"), &actor()).unwrap();
    let first = repository
        .external_reference_add(
            task.id,
            "openspec.change",
            "generic-task-api",
            "implements",
            "project",
            &actor(),
        )
        .unwrap();
    assert_eq!(first["no_op"], false);
    let repeated = repository
        .external_reference_add(
            task.id,
            "openspec.change",
            "generic-task-api",
            "implements",
            "project",
            &actor(),
        )
        .unwrap();
    assert_eq!(repeated["no_op"], true);
    assert_eq!(repository.list_versions(task.id).unwrap().len(), 2);
    assert_eq!(
        repository.external_reference_list(task.id, false).unwrap()["references"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let reference_diff = repository.compare_versions(task.id, 1, 2).unwrap();
    assert!(
        reference_diff
            .changes
            .iter()
            .any(|change| change.field == "external_references")
    );
    let reference_id = first["reference_id"].as_str().unwrap();
    assert_eq!(
        repository
            .external_reference_remove(task.id, reference_id, &actor())
            .unwrap()["no_op"],
        false
    );
    assert_eq!(
        repository
            .external_reference_remove(task.id, reference_id, &actor())
            .unwrap()["no_op"],
        true
    );
    assert!(
        repository.external_reference_list(task.id, false).unwrap()["references"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        repository.external_reference_list(task.id, true).unwrap()["references"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let versions = repository.list_versions(task.id).unwrap();
    let latest: serde_json::Value =
        serde_json::from_str(&versions.last().unwrap().snapshot_json).unwrap();
    assert!(latest["external_references"].as_array().unwrap()[0]["removed_at"].is_string());
}

#[test]
fn reviews_are_immutable_idempotent_and_increment_revision_count_once() {
    let (_root, repository) = repository();
    let task = repository.create(new_task("reviews"), &actor()).unwrap();
    let comments = vec![serde_json::json!({"path":"src/lib.rs","comment":"Adjust error handling"})];
    let first = repository
        .review_add(
            task.id,
            "review-1",
            1,
            "reviewer-a",
            "changes_requested",
            &comments,
            &actor(),
        )
        .unwrap();
    assert_eq!(first["no_op"], false);
    assert_eq!(
        repository.extension_metadata(task.id).unwrap()["revision_count"],
        1
    );
    let review_diff = repository.compare_versions(task.id, 1, 2).unwrap();
    assert!(
        review_diff
            .changes
            .iter()
            .any(|change| change.field == "revision_count")
    );
    assert!(
        review_diff
            .changes
            .iter()
            .any(|change| change.field == "reviews")
    );
    let versions_after_first_review = repository.list_versions(task.id).unwrap();
    let initial_snapshot: serde_json::Value =
        serde_json::from_str(&versions_after_first_review[0].snapshot_json).unwrap();
    let reviewed_snapshot: serde_json::Value =
        serde_json::from_str(&versions_after_first_review[1].snapshot_json).unwrap();
    assert_eq!(initial_snapshot["reviews"].as_array().unwrap().len(), 0);
    assert_eq!(reviewed_snapshot["reviews"].as_array().unwrap().len(), 1);
    assert_eq!(reviewed_snapshot["reviews"][0]["review_id"], "review-1");
    let retry = repository
        .review_add(
            task.id,
            "review-1",
            1,
            "reviewer-a",
            "changes_requested",
            &comments,
            &actor(),
        )
        .unwrap();
    assert_eq!(retry["no_op"], true);
    assert_eq!(repository.list_versions(task.id).unwrap().len(), 2);
    assert_eq!(
        repository.extension_metadata(task.id).unwrap()["revision_count"],
        1
    );
    let history_before_conflict = repository.get_history(task.id).unwrap().len();
    assert!(
        repository
            .review_add(
                task.id,
                "review-1",
                1,
                "reviewer-a",
                "approved",
                &comments,
                &actor()
            )
            .is_err()
    );
    assert_eq!(
        repository.get_history(task.id).unwrap().len(),
        history_before_conflict
    );
    let history_before_missing_version = repository.get_history(task.id).unwrap().len();
    assert!(
        repository
            .review_add(
                task.id,
                "review-missing",
                999,
                "reviewer-a",
                "approved",
                &[],
                &actor()
            )
            .is_err()
    );
    assert_eq!(
        repository.get_history(task.id).unwrap().len(),
        history_before_missing_version
    );
    repository
        .review_add(
            task.id,
            "review-2",
            1,
            "reviewer-b",
            "approved",
            &[],
            &actor(),
        )
        .unwrap();
    assert_eq!(
        repository.extension_metadata(task.id).unwrap()["revision_count"],
        1
    );
    assert_eq!(
        repository.review_list(task.id, None, None).unwrap()["reviews"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let latest = repository.list_versions(task.id).unwrap();
    let latest_snapshot: serde_json::Value =
        serde_json::from_str(&latest.last().unwrap().snapshot_json).unwrap();
    assert_eq!(latest_snapshot["reviews"].as_array().unwrap().len(), 2);
}

#[test]
fn extension_text_limits_count_unicode_characters() {
    let (_root, repository) = repository();
    let task = repository
        .create(new_task("unicode limits"), &actor())
        .unwrap();

    let namespace = "я".repeat(128);
    let value = "я".repeat(512);
    let relation = "я".repeat(128);
    repository
        .external_reference_add(task.id, &namespace, &value, &relation, "task", &actor())
        .unwrap();

    assert!(
        repository
            .external_reference_add(
                task.id,
                &format!("{namespace}я"),
                "other",
                "tracks",
                "task",
                &actor()
            )
            .is_err()
    );
    assert!(
        repository
            .external_reference_add(
                task.id,
                "other",
                &format!("{value}я"),
                "tracks",
                "task",
                &actor()
            )
            .is_err()
    );
    assert!(
        repository
            .external_reference_add(
                task.id,
                "other",
                "other",
                &format!("{relation}я"),
                "task",
                &actor()
            )
            .is_err()
    );

    let review_id = "я".repeat(128);
    let reviewer = "я".repeat(256);
    repository
        .review_add(task.id, &review_id, 1, &reviewer, "approved", &[], &actor())
        .unwrap();
    assert!(
        repository
            .review_add(
                task.id,
                &format!("{review_id}я"),
                1,
                "reviewer",
                "approved",
                &[],
                &actor()
            )
            .is_err()
    );
    assert!(
        repository
            .review_add(
                task.id,
                "review-too-long",
                1,
                &format!("{reviewer}я"),
                "approved",
                &[],
                &actor()
            )
            .is_err()
    );
}

#[test]
fn final_commit_replacement_requires_reopen_and_keeps_prior_snapshot() {
    let (_root, repository) = repository();
    let task = repository
        .create(new_task("final commit"), &actor())
        .unwrap();
    let first = "0123456789abcdef0123456789abcdef01234567";
    let second = "1123456789abcdef0123456789abcdef01234567";
    assert_eq!(
        repository
            .set_final_commit(task.id, first, &actor())
            .unwrap()["no_op"],
        false
    );
    assert_eq!(
        repository
            .set_final_commit(task.id, first, &actor())
            .unwrap()["no_op"],
        true
    );
    assert_eq!(repository.list_versions(task.id).unwrap().len(), 2);
    assert!(
        repository
            .set_final_commit(task.id, second, &actor())
            .is_err()
    );
    repository.complete(task.id, &actor()).unwrap();
    repository.reopen(task.id, None, &actor()).unwrap();
    assert_eq!(
        repository
            .set_final_commit(task.id, second, &actor())
            .unwrap()["no_op"],
        false
    );
    let versions = repository.list_versions(task.id).unwrap();
    let previous: serde_json::Value = serde_json::from_str(&versions[1].snapshot_json).unwrap();
    let current: serde_json::Value =
        serde_json::from_str(&versions.last().unwrap().snapshot_json).unwrap();
    assert_eq!(previous["final_commit_sha"], first);
    assert_eq!(current["final_commit_sha"], second);
}

#[test]
fn task_scoped_references_are_local_and_project_scope_conflicts_across_tasks() {
    let (_root, repository) = repository();
    let first = repository
        .create(new_task("first ref task"), &actor())
        .unwrap();
    let second = repository
        .create(new_task("second ref task"), &actor())
        .unwrap();

    repository
        .external_reference_add(first.id, "issue", "42", "tracks", "task", &actor())
        .unwrap();
    repository
        .external_reference_add(second.id, "issue", "42", "tracks", "task", &actor())
        .unwrap();

    repository
        .external_reference_add(
            first.id,
            "openspec.change",
            "change-1",
            "implements",
            "project",
            &actor(),
        )
        .unwrap();
    let second_history_before_conflict = repository.get_history(second.id).unwrap().len();
    assert!(
        repository
            .external_reference_add(
                second.id,
                "openspec.change",
                "change-1",
                "implements",
                "project",
                &actor()
            )
            .is_err()
    );
    assert!(
        repository
            .external_reference_add(
                second.id,
                "openspec.change",
                "change-1",
                "implements",
                "task",
                &actor()
            )
            .is_err()
    );
    assert_eq!(
        repository.get_history(second.id).unwrap().len(),
        second_history_before_conflict
    );
}

#[test]
fn review_list_supports_bounded_cursor_pagination() {
    let (_root, repository) = repository();
    let task = repository
        .create(new_task("review pagination"), &actor())
        .unwrap();
    repository
        .review_add(
            task.id,
            "review-a",
            1,
            "reviewer",
            "approved",
            &[],
            &actor(),
        )
        .unwrap();
    repository
        .review_add(
            task.id,
            "review-b",
            1,
            "reviewer",
            "approved",
            &[],
            &actor(),
        )
        .unwrap();
    let first = repository.review_list(task.id, None, Some(1)).unwrap();
    let records = first["reviews"].as_array().unwrap();
    assert_eq!(records.len(), 1);
    let cursor = records[0]["record_id"].as_i64().unwrap();
    let second = repository
        .review_list(task.id, Some(cursor), Some(1))
        .unwrap();
    assert_eq!(second["reviews"].as_array().unwrap().len(), 1);
    assert_ne!(records[0]["record_id"], second["reviews"][0]["record_id"]);
    assert!(repository.review_list(task.id, None, Some(0)).is_err());
}
