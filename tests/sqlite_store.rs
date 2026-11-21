use rusqlite::Connection;
use tempfile::tempdir;
use todoing::sqlite_store::{NewTask, SqliteStore, StoreCounts};
use serde_json::json;

fn new_task(title: &str) -> NewTask {
    NewTask {
        title: title.into(),
        status: "todo".into(),
        outcome: None,
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
    let store = SqliteStore::new(dir.path()).unwrap();

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
    let store = SqliteStore::new(dir.path()).unwrap();
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

    let store = SqliteStore::new(dir.path()).unwrap();
    assert!(store.create_task(new_task("must roll back")).is_err());
    assert_eq!(store.counts().unwrap(), StoreCounts::default());
}

#[test]
fn task_ids_are_not_reused_and_physical_delete_is_rejected() {
    let dir = tempdir().unwrap();
    let store = SqliteStore::new(dir.path()).unwrap();

    let first = store.create_task(new_task("first")).unwrap();
    let second = store.create_task(new_task("second")).unwrap();
    assert_eq!((first.id, second.id), (1, 2));

    let connection = Connection::open(store.db_path()).unwrap();
    assert!(connection
        .execute("DELETE FROM tasks WHERE id = ?1", [second.id])
        .is_err());
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
    assert_eq!(store.get_task(a.id).unwrap().unwrap().dependencies, vec![b.id]);
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
        store.get_approved_version(task.id).unwrap().unwrap().version_no,
        1
    );

    store.approve_version(task.id, 2, "reviewer").unwrap();
    assert_eq!(
        store.get_approved_version(task.id).unwrap().unwrap().version_no,
        2
    );
    assert_eq!(store.get_versions(task.id).unwrap().len(), 2);
    assert!(store
        .get_history(task.id)
        .unwrap()
        .iter()
        .any(|entry| entry.operation == "baseline_approve"));
}
