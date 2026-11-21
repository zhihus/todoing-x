use super::{Store, StoreError};
use crate::domain::ProjectId;
use tempfile::tempdir;

fn project_id() -> ProjectId {
    ProjectId::new("test-project").unwrap()
}

#[test]
fn initialize_creates_project_local_database_and_metadata() {
    let root = tempdir().unwrap();
    let id = project_id();

    let store = Store::initialize(root.path(), id.clone()).unwrap();

    assert!(store.database_path().is_file());
    assert_eq!(store.project_id(), &id);
    assert!(root.path().join(".todoing").is_dir());

    let stored_id: String = store.connection().unwrap().query_row(
        "SELECT value FROM store_metadata WHERE key = 'project_id'",
        [],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(stored_id, "test-project");
}

#[test]
fn open_checks_expected_project_identity() {
    let root = tempdir().unwrap();
    Store::initialize(root.path(), project_id()).unwrap();

    let expected = ProjectId::new("another-project").unwrap();
    assert!(matches!(
        Store::open(root.path(), Some(&expected)),
        Err(StoreError::ProjectIdMismatch { .. })
    ));

    let opened = Store::open(root.path(), None).unwrap();
    assert_eq!(opened.project_id().as_str(), "test-project");
}

#[test]
fn initialize_refuses_to_overwrite_existing_store() {
    let root = tempdir().unwrap();
    Store::initialize(root.path(), project_id()).unwrap();

    assert!(matches!(
        Store::initialize(root.path(), project_id()),
        Err(StoreError::AlreadyExists(_))
    ));
}

#[test]
fn every_connection_enforces_required_sqlite_pragmas() {
    let root = tempdir().unwrap();
    let store = Store::initialize(root.path(), project_id()).unwrap();
    let connection = store.connection().unwrap();

    let foreign_keys: i64 = connection.query_row("PRAGMA foreign_keys", [], |row| row.get(0)).unwrap();
    let synchronous: i64 = connection.query_row("PRAGMA synchronous", [], |row| row.get(0)).unwrap();
    let busy_timeout: i64 = connection.query_row("PRAGMA busy_timeout", [], |row| row.get(0)).unwrap();
    let journal_mode: String = connection.query_row("PRAGMA journal_mode", [], |row| row.get(0)).unwrap();

    assert_eq!(foreign_keys, 1);
    assert_eq!(synchronous, 2);
    assert_eq!(busy_timeout, 10_000);
    assert_eq!(journal_mode.to_ascii_lowercase(), "delete");
}

#[test]
fn verify_reports_integrity_and_schema_version() {
    let root = tempdir().unwrap();
    let store = Store::initialize(root.path(), project_id()).unwrap();

    let report = store.verify().unwrap();

    assert_eq!(report.integrity_check, "ok");
    assert_eq!(report.foreign_key_violations, 0);
    assert_eq!(report.schema_version, 1);
}

#[test]
fn database_constraints_prohibit_physical_task_deletion() {
    let root = tempdir().unwrap();
    let store = Store::initialize(root.path(), project_id()).unwrap();
    let connection = store.connection().unwrap();
    connection.execute(
        "INSERT INTO tasks(title, status_id, created_at, updated_at)
         VALUES ('task', 'todo', '2026-11-21T00:00:00Z', '2026-11-21T00:00:00Z')",
        [],
    ).unwrap();

    let result = connection.execute("DELETE FROM tasks WHERE task_id = 1", []);
    assert!(result.is_err());
}

#[test]
fn database_constraints_protect_versions_history_and_lock_generations() {
    let root = tempdir().unwrap();
    let store = Store::initialize(root.path(), project_id()).unwrap();
    let connection = store.connection().unwrap();
    connection.execute(
        "INSERT INTO tasks(title, status_id, created_at, updated_at)
         VALUES ('task', 'todo', '2026-11-21T00:00:00Z', '2026-11-21T00:00:00Z')",
        [],
    ).unwrap();
    connection.execute(
        "INSERT INTO task_versions(task_id, version_no, snapshot_json, created_at, actor_id)
         VALUES (1, 1, '{}', '2026-11-21T00:00:00Z', 'user:test')",
        [],
    ).unwrap();
    connection.execute(
        "INSERT INTO operation_history(task_id, actor_id, occurred_at, operation, outcome)
         VALUES (1, 'user:test', '2026-11-21T00:00:00Z', 'task.create', 'succeeded')",
        [],
    ).unwrap();
    connection.execute(
        "INSERT INTO task_locks(task_id, state, owner_id, token_hash, acquired_at, heartbeat_at, expires_at, fencing_generation)
         VALUES (1, 'active', 'session-1', zeroblob(32), 1000, 1000, 2000, 1)",
        [],
    ).unwrap();

    assert!(connection.execute("UPDATE task_versions SET snapshot_json = '{\"x\":1}' WHERE version_id = 1", []).is_err());
    assert!(connection.execute("DELETE FROM task_versions WHERE version_id = 1", []).is_err());
    assert!(connection.execute("UPDATE operation_history SET operation = 'changed' WHERE event_id = 1", []).is_err());
    assert!(connection.execute("DELETE FROM operation_history WHERE event_id = 1", []).is_err());

    connection.execute(
        "UPDATE task_locks
         SET state = 'released', owner_id = NULL, token_hash = NULL, expires_at = NULL, released_at = 2000
         WHERE task_id = 1",
        [],
    ).unwrap();
    assert!(connection.execute("DELETE FROM task_locks WHERE task_id = 1", []).is_err());

    let generation: i64 = connection.query_row(
        "SELECT fencing_generation FROM task_locks WHERE task_id = 1",
        [],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(generation, 1);
}

#[test]
fn foreign_key_constraints_reject_invalid_task_references() {
    let root = tempdir().unwrap();
    let store = Store::initialize(root.path(), project_id()).unwrap();
    let connection = store.connection().unwrap();

    let result = connection.execute(
        "INSERT INTO task_labels(task_id, position, label, label_key)
         VALUES (999, 0, 'orphan', 'orphan')",
        [],
    );
    assert!(result.is_err());
}

#[test]
fn open_rejects_uninitialized_project() {
    let root = tempdir().unwrap();
    assert!(matches!(Store::open(root.path(), None), Err(StoreError::NotInitialized)));
}
