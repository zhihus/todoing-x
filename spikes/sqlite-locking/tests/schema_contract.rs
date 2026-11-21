use rusqlite::{Connection, params};
use std::{fs, path::PathBuf};

fn schema_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/architecture/sqlite-schema-v1.sql")
}

fn task(connection: &Connection, id: i64) {
    connection.execute(
        "INSERT INTO tasks(task_id, title, status_id, created_at, updated_at)
         VALUES (?1, ?2, 'todo', '2026-11-21T00:00:00Z', '2026-11-21T00:00:00Z')",
        params![id, format!("task-{id}")],
    ).unwrap();
}

fn version(connection: &Connection, task_id: i64, version_no: i64) {
    connection.execute(
        "INSERT INTO task_versions(task_id, version_no, snapshot_json, created_at, actor_id)
         VALUES (?1, ?2, '{}', '2026-11-21T00:00:00Z', 'test')",
        params![task_id, version_no],
    ).unwrap();
}

#[test]
fn schema_executes_and_enforces_integrity_guards() {
    let ddl = fs::read_to_string(schema_path()).unwrap();
    let connection = Connection::open_in_memory().unwrap();
    connection.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
    connection.execute_batch(&ddl).unwrap();

    let foreign_keys: i64 = connection.query_row("PRAGMA foreign_keys", [], |row| row.get(0)).unwrap();
    assert_eq!(foreign_keys, 1);

    connection.execute(
        "INSERT INTO store_metadata(key, value) VALUES ('project_id', 'test-project')",
        [],
    ).unwrap();
    task(&connection, 1);
    task(&connection, 2);
    version(&connection, 1, 1);
    version(&connection, 2, 1);

    connection.execute(
        "INSERT INTO task_baselines(task_id, version_id, approved_at, approved_by)
         VALUES (1, 1, '2026-11-21T00:00:00Z', 'user:test')",
        [],
    ).unwrap();

    let wrong_baseline = connection.execute(
        "INSERT INTO task_baselines(task_id, version_id, approved_at, approved_by)
         VALUES (1, 2, '2026-11-21T00:00:00Z', 'user:test')",
        [],
    );
    assert!(wrong_baseline.is_err(), "a baseline cannot reference another task's version");

    let task_delete = connection.execute("DELETE FROM tasks WHERE task_id = 1", []);
    assert!(task_delete.is_err(), "physical task deletion must be blocked");

    let version_update = connection.execute(
        "UPDATE task_versions SET snapshot_json = '{\"changed\":true}' WHERE task_id = 1",
        [],
    );
    assert!(version_update.is_err(), "task versions must be immutable");

    connection.execute(
        "INSERT INTO operation_history(task_id, actor_id, occurred_at, operation, outcome)
         VALUES (1, 'agent:test', '2026-11-21T00:00:00Z', 'task.create', 'succeeded')",
        [],
    ).unwrap();
    let history_update = connection.execute(
        "UPDATE operation_history SET operation = 'changed' WHERE task_id = 1",
        [],
    );
    assert!(history_update.is_err(), "operation history must be append-only");

    let invalid_sha = connection.execute(
        "INSERT INTO task_commit_refs(task_id, position, sha) VALUES (1, 0, 'not-a-sha')",
        [],
    );
    assert!(invalid_sha.is_err(), "commit references must be full lowercase SHA values");

    let self_dependency = connection.execute(
        "INSERT INTO task_dependencies(task_id, depends_on_task_id, created_at)
         VALUES (1, 1, '2026-11-21T00:00:00Z')",
        [],
    );
    assert!(self_dependency.is_err(), "self-dependencies must be blocked");

    connection.execute(
        "INSERT INTO task_locks(task_id, state, owner_id, token_hash, acquired_at, heartbeat_at, expires_at, fencing_generation)
         VALUES (1, 'active', 'session-1', zeroblob(32), 1000, 1000, 2000, 1)",
        [],
    ).unwrap();
    connection.execute(
        "UPDATE task_locks
         SET state = 'released', owner_id = NULL, token_hash = NULL, expires_at = NULL, released_at = 2000
         WHERE task_id = 1",
        [],
    ).unwrap();
    let lock_delete = connection.execute("DELETE FROM task_locks WHERE task_id = 1", []);
    assert!(lock_delete.is_err(), "lock fencing generations must be retained");

    let integrity: String = connection.query_row("PRAGMA integrity_check", [], |row| row.get(0)).unwrap();
    assert_eq!(integrity, "ok");
    let foreign_key_errors: i64 = connection.query_row(
        "SELECT COUNT(*) FROM pragma_foreign_key_check",
        [],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(foreign_key_errors, 0);
}
