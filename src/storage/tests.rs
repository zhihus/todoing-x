use super::{Store, StoreError};
use crate::config::ProjectConfig;
use crate::domain::{ProjectId, TaskId};
use crate::storage::TaskRepository;
use rusqlite::Connection;
use sha2::Digest;
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
    assert_eq!(report.schema_version, 3);
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

#[test]
fn open_rejects_unknown_schema_versions() {
    let root = tempdir().unwrap();
    let store = Store::initialize(root.path(), project_id()).unwrap();
    store.connection().unwrap().execute(
        "UPDATE store_metadata SET value = '4' WHERE key = 'schema_version'",
        [],
    ).unwrap();

    assert!(matches!(
        Store::open(root.path(), None),
        Err(StoreError::UnsupportedSchemaVersion(4))
    ));
}

#[test]
fn verify_rejects_migration_checksum_drift() {
    let root = tempdir().unwrap();
    let store = Store::initialize(root.path(), project_id()).unwrap();
    store.connection().unwrap().execute(
        "UPDATE schema_migrations SET checksum = 'invalid' WHERE version = 3",
        [],
    ).unwrap();

    assert!(matches!(
        store.verify(),
        Err(StoreError::MigrationChecksumMismatch { version: 3 })
    ));
}

#[test]
fn initial_migration_matches_the_documented_schema() {
    const DOCUMENTED: &str = include_str!("../../docs/architecture/sqlite-schema-v1.sql");
    const MIGRATION: &str = include_str!("../../migrations/0001_initial.sql");
    assert_eq!(MIGRATION, DOCUMENTED);
}

#[test]
fn verify_rejects_project_identity_drift() {
    let root = tempdir().unwrap();
    let store = Store::initialize(root.path(), project_id()).unwrap();
    store.connection().unwrap().execute(
        "UPDATE store_metadata SET value = 'different-project' WHERE key = 'project_id'",
        [],
    ).unwrap();

    assert!(matches!(
        store.verify(),
        Err(StoreError::ProjectIdMismatch { .. })
    ));
}

#[test]
fn verify_detects_foreign_key_violations_created_by_external_writer() {
    let root = tempdir().unwrap();
    let store = Store::initialize(root.path(), project_id()).unwrap();

    let external = Connection::open(store.database_path()).unwrap();
    external.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
    external.execute(
        "INSERT INTO task_labels(task_id, position, label, label_key)
         VALUES (999, 0, 'orphan', 'orphan')",
        [],
    ).unwrap();
    drop(external);

    assert!(matches!(
        store.verify(),
        Err(StoreError::ForeignKeyViolations(1))
    ));
}

#[test]
fn verify_rejects_missing_protection_triggers() {
    let root = tempdir().unwrap();
    let store = Store::initialize(root.path(), project_id()).unwrap();
    store.connection().unwrap().execute_batch("DROP TRIGGER tasks_no_delete;").unwrap();

    assert!(matches!(
        store.verify(),
        Err(StoreError::InvalidMetadata(message)) if message.contains("tasks_no_delete")
    ));
}


#[test]
fn online_backup_snapshot_is_verified_hashed_and_respects_overwrite_policy() {
    let root = tempdir().unwrap();
    let store = Store::initialize(root.path(), project_id()).unwrap();
    store.connection().unwrap().execute(
        "INSERT INTO tasks(title, status_id, created_at, updated_at)
         VALUES ('snapshot task', 'todo', '2026-11-21T00:00:00Z', '2026-11-21T00:00:00Z')",
        [],
    ).unwrap();

    let report = store.snapshot("backup.db", false).unwrap();
    assert!(report.path.is_file());
    assert!(report.size_bytes > 0);
    assert_eq!(report.sha256.len(), 64);

    let snapshot = Connection::open(&report.path).unwrap();
    let title: String = snapshot.query_row("SELECT title FROM tasks WHERE task_id = 1", [], |row| row.get(0)).unwrap();
    assert_eq!(title, "snapshot task");
    drop(snapshot);

    assert!(matches!(
        store.snapshot("backup.db", false),
        Err(StoreError::AlreadyExists(_))
    ));
    assert!(store.snapshot("backup.db", true).is_ok());
}

#[test]
fn online_backup_snapshot_refuses_active_task_locks() {
    let root = tempdir().unwrap();
    let store = Store::initialize(root.path(), project_id()).unwrap();
    let connection = store.connection().unwrap();
    connection.execute(
        "INSERT INTO tasks(title, status_id, created_at, updated_at)
         VALUES ('locked task', 'todo', '2026-11-21T00:00:00Z', '2026-11-21T00:00:00Z')",
        [],
    ).unwrap();
    connection.execute(
        "INSERT INTO task_locks(task_id, state, owner_id, token_hash, acquired_at, heartbeat_at, expires_at, fencing_generation)
         VALUES (1, 'active', 'session-1', zeroblob(32), 1000, 1000, 2000, 1)",
        [],
    ).unwrap();
    drop(connection);

    assert!(matches!(store.snapshot("blocked.db", false), Err(StoreError::TaskLocked)));
    assert!(!root.path().join("blocked.db").exists());
}

#[test]
fn online_backup_snapshot_refuses_the_active_database_as_destination() {
    let root = tempdir().unwrap();
    let store = Store::initialize(root.path(), project_id()).unwrap();

    assert!(matches!(
        store.snapshot(store.database_path(), true),
        Err(StoreError::InvalidTask(_))
    ));
}


#[test]
fn opening_a_v1_store_applies_all_migrations_without_changing_existing_tasks() {
    let root = tempdir().unwrap();
    let todoing_dir = root.path().join(".todoing");
    std::fs::create_dir_all(&todoing_dir).unwrap();
    let database_path = todoing_dir.join("todoing.db");
    let connection = Connection::open(&database_path).unwrap();
    connection.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL;").unwrap();
    connection.execute_batch(include_str!("../../migrations/0001_initial.sql")).unwrap();
    let created_at = "2026-11-22T00:00:00Z";
    connection.execute(
        "INSERT INTO store_metadata(key, value) VALUES ('schema_version', '1')",
        [],
    ).unwrap();
    connection.execute(
        "INSERT INTO store_metadata(key, value) VALUES ('project_id', 'task-repository-tests')",
        [],
    ).unwrap();
    connection.execute(
        "INSERT INTO store_metadata(key, value) VALUES ('created_at', ?1)",
        [created_at],
    ).unwrap();
    connection.execute(
        "INSERT INTO schema_migrations(version, checksum, applied_at) VALUES (1, ?1, ?2)",
        [hex::encode(sha2::Sha256::digest(include_str!("../../migrations/0001_initial.sql").as_bytes())), created_at.to_owned()],
    ).unwrap();
    connection.execute(
        "INSERT INTO tasks(task_id, title, status_id, outcome, body, created_at, updated_at)
         VALUES (42, 'legacy v1 task', 'todo', NULL, 'preserved body', ?1, ?1)",
        [created_at],
    ).unwrap();
    drop(connection);

    let store = Store::open(root.path(), Some(&ProjectId::new("task-repository-tests").unwrap())).unwrap();
    let report = store.verify().unwrap();
    assert_eq!(report.schema_version, 3);

    let migrated = store.connection().unwrap();
    let archive_columns: Vec<String> = {
        let mut statement = migrated.prepare("PRAGMA table_info(tasks)").unwrap();
        statement.query_map([], |row| row.get::<_, String>(1)).unwrap()
            .collect::<Result<Vec<_>, _>>().unwrap()
    };
    assert!(archive_columns.contains(&"archived_at".to_owned()));
    assert!(archive_columns.contains(&"archived_by".to_owned()));
    assert!(archive_columns.contains(&"revision_count".to_owned()));
    assert!(archive_columns.contains(&"final_commit_sha".to_owned()));

    let values: (Option<String>, Option<String>, String, String) = migrated.query_row(
        "SELECT archived_at, archived_by, title, body FROM tasks WHERE task_id = 42",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    ).unwrap();
    assert_eq!(values, (None, None, "legacy v1 task".into(), "preserved body".into()));
    let extension_table_count: i64 = migrated.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN ('task_external_references', 'task_reviews')",
        [],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(extension_table_count, 2);
}


#[test]
fn opening_a_v2_store_applies_extensions_migration_and_preserves_existing_data() {
    let root = tempdir().unwrap();
    let todoing_dir = root.path().join(".todoing");
    std::fs::create_dir_all(&todoing_dir).unwrap();
    let database_path = todoing_dir.join("todoing.db");
    let connection = Connection::open(&database_path).unwrap();
    connection.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL;").unwrap();
    connection.execute_batch(include_str!("../../migrations/0001_initial.sql")).unwrap();
    let created_at = "2026-11-23T00:00:00Z";
    connection.execute(
        "INSERT INTO store_metadata(key, value) VALUES ('schema_version', '1')",
        [],
    ).unwrap();
    connection.execute(
        "INSERT INTO store_metadata(key, value) VALUES ('project_id', 'task-repository-tests')",
        [],
    ).unwrap();
    connection.execute(
        "INSERT INTO store_metadata(key, value) VALUES ('created_at', ?1)",
        [created_at],
    ).unwrap();
    connection.execute(
        "INSERT INTO schema_migrations(version, checksum, applied_at) VALUES (1, ?1, ?2)",
        [hex::encode(sha2::Sha256::digest(include_str!("../../migrations/0001_initial.sql").as_bytes())), created_at.to_owned()],
    ).unwrap();
    connection.execute_batch(include_str!("../../migrations/0002_task_archive.sql")).unwrap();
    connection.execute(
        "UPDATE store_metadata SET value = '2' WHERE key = 'schema_version'",
        [],
    ).unwrap();
    connection.execute(
        "INSERT INTO schema_migrations(version, checksum, applied_at) VALUES (2, ?1, ?2)",
        [hex::encode(sha2::Sha256::digest(include_str!("../../migrations/0002_task_archive.sql").as_bytes())), created_at.to_owned()],
    ).unwrap();

    connection.execute(
        "INSERT INTO tasks(task_id, title, status_id, outcome, body, created_at, updated_at, archived_at, archived_by)
         VALUES (42, 'legacy v2 task', 'todo', NULL, 'preserved v2 body', ?1, ?1, '2026-11-22T12:00:00Z', 'user:archive')",
        [created_at],
    ).unwrap();
    connection.execute(
        "INSERT INTO tasks(task_id, title, status_id, outcome, body, created_at, updated_at)
         VALUES (43, 'dependent v2 task', 'in_progress', NULL, 'dependent body', ?1, ?1)",
        [created_at],
    ).unwrap();
    connection.execute(
        "INSERT INTO task_labels(task_id, position, label, label_key) VALUES (42, 0, 'Legacy', 'legacy')",
        [],
    ).unwrap();
    connection.execute(
        "INSERT INTO task_commit_refs(task_id, position, sha) VALUES (42, 0, ?1)",
        ["0123456789abcdef0123456789abcdef01234567"],
    ).unwrap();
    connection.execute(
        "INSERT INTO task_dependencies(task_id, depends_on_task_id, created_at) VALUES (43, 42, ?1)",
        [created_at],
    ).unwrap();
    let legacy_snapshot = serde_json::json!({
        "task_id": 42,
        "title": "legacy v2 task",
        "status_id": "todo",
        "outcome": null,
        "body": "preserved v2 body",
        "created_at": created_at,
        "updated_at": created_at,
        "archived_at": "2026-11-22T12:00:00Z",
        "archived_by": "user:archive",
        "labels": ["Legacy"],
        "commit_refs": ["0123456789abcdef0123456789abcdef01234567"],
        "dependencies": []
    }).to_string();
    connection.execute(
        "INSERT INTO task_versions(task_id, version_no, snapshot_json, created_at, actor_id)
         VALUES (42, 1, ?1, ?2, 'user:legacy')",
        rusqlite::params![legacy_snapshot, created_at],
    ).unwrap();
    let version_id: i64 = connection.query_row(
        "SELECT version_id FROM task_versions WHERE task_id = 42 AND version_no = 1",
        [],
        |row| row.get(0),
    ).unwrap();
    connection.execute(
        "INSERT INTO task_baselines(task_id, version_id, approved_at, approved_by) VALUES (42, ?1, ?2, 'user:approver')",
        rusqlite::params![version_id, created_at],
    ).unwrap();
    connection.execute(
        "INSERT INTO operation_history(task_id, actor_id, occurred_at, operation, outcome)
         VALUES (42, 'user:legacy', ?1, 'task.create', 'succeeded')",
        [created_at],
    ).unwrap();
    connection.execute(
        "INSERT INTO task_locks(task_id, state, owner_id, token_hash, acquired_at, heartbeat_at, expires_at, fencing_generation)
         VALUES (42, 'active', 'legacy-owner', zeroblob(32), 1000, 1000, 2000, 7)",
        [],
    ).unwrap();
    drop(connection);

    let store = Store::open(root.path(), Some(&ProjectId::new("task-repository-tests").unwrap())).unwrap();
    let report = store.verify().unwrap();
    assert_eq!(report.schema_version, 3);

    let repository = TaskRepository::new(store.clone(), ProjectConfig::default()).unwrap();
    let legacy_version_diff = repository.compare_versions(
        TaskId::new(42).unwrap(), 1, 1
    ).unwrap();
    assert!(legacy_version_diff.changes.is_empty());

    let migrated = store.connection().unwrap();
    let task: (String, String, i64, Option<String>, Option<String>, Option<String>) = migrated.query_row(
        "SELECT title, body, revision_count, final_commit_sha, archived_at, archived_by FROM tasks WHERE task_id = 42",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
    ).unwrap();
    assert_eq!(task, (
        "legacy v2 task".into(),
        "preserved v2 body".into(),
        0,
        None,
        Some("2026-11-22T12:00:00Z".into()),
        Some("user:archive".into()),
    ));

    let label: String = migrated.query_row(
        "SELECT label FROM task_labels WHERE task_id = 42 AND position = 0",
        [],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(label, "Legacy");
    let commit_sha: String = migrated.query_row(
        "SELECT sha FROM task_commit_refs WHERE task_id = 42 AND position = 0",
        [],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(commit_sha, "0123456789abcdef0123456789abcdef01234567");
    let dependency: i64 = migrated.query_row(
        "SELECT depends_on_task_id FROM task_dependencies WHERE task_id = 43",
        [],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(dependency, 42);

    let baseline_count: i64 = migrated.query_row(
        "SELECT COUNT(*) FROM task_baselines b JOIN task_versions v
         ON v.task_id = b.task_id AND v.version_id = b.version_id
         WHERE b.task_id = 42 AND v.version_no = 1 AND b.approved_by = 'user:approver'",
        [],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(baseline_count, 1);
    let history_count: i64 = migrated.query_row(
        "SELECT COUNT(*) FROM operation_history WHERE task_id = 42 AND operation = 'task.create'",
        [],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(history_count, 1);
    let lock: (String, String, i64, i64) = migrated.query_row(
        "SELECT state, owner_id, length(token_hash), fencing_generation FROM task_locks WHERE task_id = 42",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    ).unwrap();
    assert_eq!(lock, ("active".into(), "legacy-owner".into(), 32, 7));

    let migration_count: i64 = migrated.query_row(
        "SELECT COUNT(*) FROM schema_migrations WHERE version = 3",
        [],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(migration_count, 1);
    let extension_table_count: i64 = migrated.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table'
         AND name IN ('task_external_references', 'task_reviews')",
        [],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(extension_table_count, 2);
}

#[test]
fn failed_archive_schema_migration_rolls_back_all_ddl_and_version_changes() {
    let root = tempdir().unwrap();
    let todoing_dir = root.path().join(".todoing");
    std::fs::create_dir_all(&todoing_dir).unwrap();
    let database_path = todoing_dir.join("todoing.db");
    let connection = Connection::open(&database_path).unwrap();
    connection.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL;").unwrap();
    connection.execute_batch(include_str!("../../migrations/0001_initial.sql")).unwrap();
    let created_at = "2026-11-22T00:00:00Z";
    connection.execute(
        "INSERT INTO store_metadata(key, value) VALUES ('schema_version', '1')",
        [],
    ).unwrap();
    connection.execute(
        "INSERT INTO store_metadata(key, value) VALUES ('project_id', 'task-repository-tests')",
        [],
    ).unwrap();
    connection.execute(
        "INSERT INTO store_metadata(key, value) VALUES ('created_at', ?1)",
        [created_at],
    ).unwrap();
    connection.execute(
        "INSERT INTO schema_migrations(version, checksum, applied_at) VALUES (1, ?1, ?2)",
        [hex::encode(sha2::Sha256::digest(include_str!("../../migrations/0001_initial.sql").as_bytes())), created_at.to_owned()],
    ).unwrap();
    connection.execute_batch("CREATE INDEX idx_tasks_archive ON tasks(task_id);").unwrap();
    drop(connection);

    assert!(Store::open(root.path(), Some(&ProjectId::new("task-repository-tests").unwrap())).is_err());

    let connection = Connection::open(&database_path).unwrap();
    let version: String = connection.query_row(
        "SELECT value FROM store_metadata WHERE key = 'schema_version'",
        [],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(version, "1");
    let columns: Vec<String> = {
        let mut statement = connection.prepare("PRAGMA table_info(tasks)").unwrap();
        statement.query_map([], |row| row.get::<_, String>(1)).unwrap()
            .collect::<Result<Vec<_>, _>>().unwrap()
    };
    assert!(!columns.contains(&"archived_at".to_owned()));
    assert!(!columns.contains(&"archived_by".to_owned()));
    let preserved_index: i64 = connection.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'idx_tasks_archive'",
        [],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(preserved_index, 1);
}
