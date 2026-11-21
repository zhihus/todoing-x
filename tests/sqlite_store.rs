use rusqlite::Connection;
use serde_json::json;
use tempfile::tempdir;
use todoing::sqlite_store::{NewTask, SqliteStore, StoreCounts};

fn new_task(title: &str) -> NewTask {
    NewTask {
        title: title.into(),
        status: "todo".into(),
        body: format!("body for {title}"),
        labels: vec!["mvp".into()],
        commits: Vec::new(),
        metadata: json!({}),
        actor: "test".into(),
    }
}

#[test]
fn create_task_commits_task_version_and_history_together() {
    let dir = tempdir().unwrap();
    let mut store = SqliteStore::new(dir.path()).unwrap();

    let task = store.create_task(new_task("first task")).unwrap();

    assert_eq!(task.id, 1);
    assert_eq!(task.title, "first task");
    assert_eq!(task.status, "todo");
    assert_eq!(
        store.counts().unwrap(),
        StoreCounts {
            tasks: 1,
            versions: 1,
            history: 1
        }
    );
    assert_eq!(store.get_versions(task.id).unwrap().len(), 1);
    assert_eq!(store.get_history(task.id).unwrap().len(), 1);
    store.integrity_check().unwrap();
}

#[test]
fn failed_history_insert_rolls_back_task_and_version() {
    let dir = tempdir().unwrap();
    let mut store = SqliteStore::new(dir.path()).unwrap();
    let db_path = store.db_path().to_path_buf();
    drop(store);

    let connection = Connection::open(&db_path).unwrap();
    connection
        .execute_batch(
            "CREATE TRIGGER reject_history
             BEFORE INSERT ON operation_history
             BEGIN
                 SELECT RAISE(ABORT, 'injected history failure');
             END;",
        )
        .unwrap();
    drop(connection);

    let mut store = SqliteStore::new(dir.path()).unwrap();
    assert!(store.create_task(new_task("must roll back")).is_err());
    assert_eq!(store.counts().unwrap(), StoreCounts::default());
}

#[test]
fn task_ids_are_not_reused_and_physical_delete_is_rejected() {
    let dir = tempdir().unwrap();
    let mut store = SqliteStore::new(dir.path()).unwrap();

    let first = store.create_task(new_task("first")).unwrap();
    let second = store.create_task(new_task("second")).unwrap();
    assert_eq!((first.id, second.id), (1, 2));

    let connection = Connection::open(store.db_path()).unwrap();
    assert!(
        connection
            .execute("DELETE FROM tasks WHERE id = ?1", [second.id])
            .is_err()
    );
    drop(connection);

    let third = store.create_task(new_task("third")).unwrap();
    assert_eq!(third.id, 3);
}

#[test]
fn changed_save_creates_version_and_noop_save_does_not() {
    let dir = tempdir().unwrap();
    let mut store = SqliteStore::new(dir.path()).unwrap();
    let mut task = store.create_task(new_task("before")).unwrap();

    task.title = "after".into();
    let saved = store.update_task(&task, "test").unwrap();
    assert_eq!(saved.title, "after");
    assert_eq!(store.get_versions(task.id).unwrap().len(), 2);

    let saved_again = store.update_task(&saved, "test").unwrap();
    assert_eq!(saved_again.title, "after");
    assert_eq!(store.get_versions(task.id).unwrap().len(), 2);
    assert_eq!(store.get_history(task.id).unwrap().len(), 2);
}

#[test]
fn dependency_graph_rejects_direct_and_indirect_cycles() {
    let dir = tempdir().unwrap();
    let mut store = SqliteStore::new(dir.path()).unwrap();
    let a = store.create_task(new_task("A")).unwrap();
    let b = store.create_task(new_task("B")).unwrap();
    let c = store.create_task(new_task("C")).unwrap();

    store.add_dependency(a.id, b.id, "test").unwrap();
    store.add_dependency(b.id, c.id, "test").unwrap();

    assert!(store.add_dependency(c.id, a.id, "test").is_err());
    assert!(store.add_dependency(a.id, a.id, "test").is_err());
    assert_eq!(
        store.get_task(a.id).unwrap().unwrap().dependencies,
        vec![b.id]
    );
}

#[test]
fn approved_baseline_points_to_a_specific_immutable_version() {
    let dir = tempdir().unwrap();
    let mut store = SqliteStore::new(dir.path()).unwrap();
    let mut task = store.create_task(new_task("v1")).unwrap();
    task.title = "v2".into();
    let task = store.update_task(&task, "test").unwrap();

    store.approve_version(task.id, 1, "reviewer").unwrap();
    assert_eq!(
        store
            .get_approved_version(task.id)
            .unwrap()
            .unwrap()
            .version_no,
        1
    );

    store.approve_version(task.id, 2, "reviewer").unwrap();
    assert_eq!(
        store
            .get_approved_version(task.id)
            .unwrap()
            .unwrap()
            .version_no,
        2
    );
    assert_eq!(store.get_versions(task.id).unwrap().len(), 2);
    assert!(
        store
            .get_history(task.id)
            .unwrap()
            .iter()
            .any(|entry| entry.operation == "baseline_approve")
    );
}

#[test]
fn rejects_unconfigured_statuses_commit_ids_and_empty_actors() {
    let dir = tempdir().unwrap();
    let mut store = SqliteStore::new(dir.path()).unwrap();

    let mut invalid_status = new_task("invalid status");
    invalid_status.status = "finished".into();
    assert!(store.create_task(invalid_status).is_err());

    let mut abbreviated_commit = new_task("abbreviated commit");
    abbreviated_commit.commits = vec!["abc123".into()];
    assert!(store.create_task(abbreviated_commit).is_err());

    let mut empty_actor = new_task("empty actor");
    empty_actor.actor.clear();
    assert!(store.create_task(empty_actor).is_err());

    assert_eq!(store.counts().unwrap(), StoreCounts::default());
}

#[test]
fn configured_status_transitions_set_and_clear_outcomes() {
    let dir = tempdir().unwrap();
    let mut store = SqliteStore::new(dir.path()).unwrap();
    let mut task = store.create_task(new_task("workflow")).unwrap();

    task.status = "done".into();
    assert!(store.update_task(&task, "test").is_err());

    task.status = "in_progress".into();
    task = store.update_task(&task, "test").unwrap();
    assert_eq!(task.outcome, None);

    task.status = "done".into();
    task = store.update_task(&task, "test").unwrap();
    assert_eq!(task.outcome.as_deref(), Some("completed"));

    task.status = "in_progress".into();
    task = store.update_task(&task, "test").unwrap();
    assert_eq!(task.outcome, None);
    assert_eq!(store.get_versions(task.id).unwrap().len(), 4);
}

#[test]
fn configuration_changes_do_not_rewrite_existing_task_outcomes() {
    let dir = tempdir().unwrap();
    let mut store = SqliteStore::new(dir.path()).unwrap();
    let mut task = store.create_task(new_task("stable outcome")).unwrap();
    task.status = "in_progress".into();
    task = store.update_task(&task, "test").unwrap();
    task.status = "done".into();
    task = store.update_task(&task, "test").unwrap();
    assert_eq!(task.outcome.as_deref(), Some("completed"));
    drop(store);

    let config_path = dir.path().join(".todoing").join("config.toml");
    let config = std::fs::read_to_string(&config_path).unwrap();
    std::fs::write(
        &config_path,
        config.replace("outcome = \"completed\"", "outcome = \"cancelled\""),
    )
    .unwrap();

    let mut store = SqliteStore::new(dir.path()).unwrap();
    let existing = store.get_task(task.id).unwrap().unwrap();
    assert_eq!(existing.outcome.as_deref(), Some("completed"));

    let mut new_task = new_task("new outcome");
    new_task.status = "done".into();
    let created = store.create_task(new_task).unwrap();
    assert_eq!(created.outcome.as_deref(), Some("cancelled"));
}

#[test]
fn compare_versions_returns_stable_field_changes_without_mutating_snapshots() {
    let dir = tempdir().unwrap();
    let mut store = SqliteStore::new(dir.path()).unwrap();
    let mut task = store.create_task(new_task("before")).unwrap();

    task.title = "after".into();
    task.body = "updated body".into();
    task.labels = vec!["mvp".into(), "reviewed".into()];
    task.metadata = json!({"source": "version-two"});
    store.update_task(&task, "test").unwrap();

    let versions_before = store.get_versions(task.id).unwrap();
    let diff = store.compare_versions(task.id, 1, 2).unwrap();

    assert_eq!(diff.task_id, task.id);
    assert_eq!(diff.from_version, 1);
    assert_eq!(diff.to_version, 2);
    assert_eq!(
        diff.changes
            .iter()
            .map(|change| change.field.as_str())
            .collect::<Vec<_>>(),
        vec!["title", "body", "labels", "metadata"]
    );
    assert_eq!(diff.changes[0].before, json!("before"));
    assert_eq!(diff.changes[0].after, json!("after"));
    assert_eq!(diff.changes[1].before, json!("body for before"));
    assert_eq!(diff.changes[1].after, json!("updated body"));
    assert_eq!(diff.changes[2].before, json!(["mvp"]));
    assert_eq!(diff.changes[2].after, json!(["mvp", "reviewed"]));
    assert_eq!(diff.changes[3].after, json!({"source": "version-two"}));
    assert_eq!(store.get_versions(task.id).unwrap(), versions_before);

    let reverse = store.compare_versions(task.id, 2, 1).unwrap();
    assert_eq!(reverse.changes[0].before, json!("after"));
    assert_eq!(reverse.changes[0].after, json!("before"));
}

#[test]
fn comparing_a_version_with_itself_returns_no_changes() {
    let dir = tempdir().unwrap();
    let mut store = SqliteStore::new(dir.path()).unwrap();
    let task = store.create_task(new_task("unchanged")).unwrap();

    let diff = store.compare_versions(task.id, 1, 1).unwrap();

    assert!(diff.changes.is_empty());
}

#[test]
fn comparing_a_missing_version_returns_a_clear_error() {
    let dir = tempdir().unwrap();
    let mut store = SqliteStore::new(dir.path()).unwrap();
    let task = store.create_task(new_task("existing")).unwrap();

    let error = store.compare_versions(task.id, 1, 99).unwrap_err();

    assert!(error.contains("Version 99"));
    assert!(error.contains(&format!("#{}", task.id)));
}

#[test]
fn version_comparison_includes_dependency_changes() {
    let dir = tempdir().unwrap();
    let mut store = SqliteStore::new(dir.path()).unwrap();
    let dependent = store.create_task(new_task("dependent")).unwrap();
    let prerequisite = store.create_task(new_task("prerequisite")).unwrap();

    store
        .add_dependency(dependent.id, prerequisite.id, "test")
        .unwrap();

    let diff = store.compare_versions(dependent.id, 1, 2).unwrap();

    assert_eq!(diff.changes.len(), 1);
    assert_eq!(diff.changes[0].field, "dependencies");
    assert_eq!(diff.changes[0].before, json!([]));
    assert_eq!(diff.changes[0].after, json!([prerequisite.id]));
}
