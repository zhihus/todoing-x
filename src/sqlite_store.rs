use crate::config::ProjectConfig;
use chrono::{SecondsFormat, Utc};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

const SCHEMA_VERSION: i64 = 1;
const DATABASE_FILE: &str = "todoing.db";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct NewTask {
    pub title: String,
    pub status: String,
    pub body: String,
    pub labels: Vec<String>,
    pub commits: Vec<String>,
    pub metadata: Value,
    pub actor: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct StoredTask {
    pub id: i64,
    pub title: String,
    pub status: String,
    pub outcome: Option<String>,
    pub body: String,
    pub created_at: String,
    pub updated_at: String,
    pub labels: Vec<String>,
    pub commits: Vec<String>,
    pub dependencies: Vec<i64>,
    pub metadata: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TaskVersion {
    pub version_no: i64,
    pub snapshot: StoredTask,
    pub created_at: String,
    pub actor: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct HistoryEntry {
    pub event_id: i64,
    pub operation: String,
    pub actor: String,
    pub occurred_at: String,
    pub before_json: Option<String>,
    pub after_json: Option<String>,
    pub result: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StoreCounts {
    pub tasks: u64,
    pub versions: u64,
    pub history: u64,
}

pub struct SqliteStore {
    connection: Connection,
    db_path: PathBuf,
    config: ProjectConfig,
}

impl SqliteStore {
    pub fn new(project_root: impl AsRef<Path>) -> Result<Self, String> {
        let todoing_dir = project_root.as_ref().join(".todoing");
        fs::create_dir_all(&todoing_dir).map_err(|error| {
            format!(
                "Cannot create Todoing directory {}: {error}",
                todoing_dir.display()
            )
        })?;
        let config = ProjectConfig::load_or_create(&todoing_dir)?;
        let db_path = todoing_dir.join(DATABASE_FILE);
        Self::open_database(db_path, config)
    }

    fn open_database(path: impl AsRef<Path>, config: ProjectConfig) -> Result<Self, String> {
        config.validate()?;
        let db_path = path.as_ref().to_path_buf();
        if let Some(parent) = db_path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "Cannot create database directory {}: {error}",
                    parent.display()
                )
            })?;
        }

        let mut connection = Connection::open(&db_path).map_err(|error| {
            format!(
                "Cannot open Todoing database {}: {error}",
                db_path.display()
            )
        })?;
        connection
            .busy_timeout(Duration::from_secs(30))
            .map_err(|error| error.to_string())?;
        connection
            .pragma_update(None, "foreign_keys", true)
            .map_err(|error| error.to_string())?;
        connection
            .pragma_update(None, "journal_mode", "WAL")
            .map_err(|error| format!("Cannot enable SQLite WAL mode: {error}"))?;
        connection
            .pragma_update(None, "synchronous", "FULL")
            .map_err(|error| error.to_string())?;

        Self::initialize_schema(&mut connection)?;
        Ok(Self {
            connection,
            db_path,
            config,
        })
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub fn config(&self) -> &ProjectConfig {
        &self.config
    }

    fn initialize_schema(connection: &mut Connection) -> Result<(), String> {
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(|error| error.to_string())?;

        match version {
            SCHEMA_VERSION => return Ok(()),
            0 => {}
            other => {
                return Err(format!(
                    "Unsupported Todoing database schema version {other}; supported version is {SCHEMA_VERSION}"
                ));
            }
        }

        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        transaction
            .execute_batch(
                "
                CREATE TABLE project_meta (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                );

                CREATE TABLE tasks (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    title TEXT NOT NULL CHECK(length(trim(title)) > 0),
                    status TEXT NOT NULL CHECK(length(trim(status)) > 0),
                    outcome TEXT CHECK(outcome IS NULL OR outcome IN ('completed', 'cancelled')),
                    body TEXT NOT NULL DEFAULT '',
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    metadata_json TEXT NOT NULL DEFAULT '{}',
                    legacy_frontmatter TEXT
                );

                CREATE TABLE task_labels (
                    task_id INTEGER NOT NULL REFERENCES tasks(id) ON DELETE RESTRICT,
                    label TEXT NOT NULL CHECK(length(trim(label)) > 0),
                    position INTEGER NOT NULL,
                    PRIMARY KEY(task_id, label),
                    UNIQUE(task_id, position)
                );

                CREATE TABLE task_git_commits (
                    task_id INTEGER NOT NULL REFERENCES tasks(id) ON DELETE RESTRICT,
                    sha TEXT NOT NULL CHECK(length(trim(sha)) > 0),
                    position INTEGER NOT NULL,
                    PRIMARY KEY(task_id, sha),
                    UNIQUE(task_id, position)
                );

                CREATE TABLE task_dependencies (
                    task_id INTEGER NOT NULL REFERENCES tasks(id) ON DELETE RESTRICT,
                    depends_on_id INTEGER NOT NULL REFERENCES tasks(id) ON DELETE RESTRICT,
                    created_at TEXT NOT NULL,
                    PRIMARY KEY(task_id, depends_on_id),
                    CHECK(task_id <> depends_on_id)
                );

                CREATE TABLE task_versions (
                    version_id INTEGER PRIMARY KEY AUTOINCREMENT,
                    task_id INTEGER NOT NULL REFERENCES tasks(id) ON DELETE RESTRICT,
                    version_no INTEGER NOT NULL,
                    snapshot_json TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    actor TEXT NOT NULL,
                    UNIQUE(task_id, version_no),
                    UNIQUE(task_id, version_id)
                );

                CREATE TABLE operation_history (
                    event_id INTEGER PRIMARY KEY AUTOINCREMENT,
                    task_id INTEGER NOT NULL REFERENCES tasks(id) ON DELETE RESTRICT,
                    actor TEXT NOT NULL,
                    occurred_at TEXT NOT NULL,
                    operation TEXT NOT NULL,
                    before_json TEXT,
                    after_json TEXT,
                    result TEXT NOT NULL
                );

                CREATE TABLE task_version_approvals (
                    task_id INTEGER PRIMARY KEY REFERENCES tasks(id) ON DELETE RESTRICT,
                    version_id INTEGER NOT NULL,
                    approved_by TEXT NOT NULL,
                    approved_at TEXT NOT NULL,
                    FOREIGN KEY(task_id, version_id)
                        REFERENCES task_versions(task_id, version_id) ON DELETE RESTRICT
                );

                CREATE TABLE task_locks (
                    task_id INTEGER PRIMARY KEY REFERENCES tasks(id) ON DELETE RESTRICT,
                    owner_id TEXT NOT NULL,
                    token_hash TEXT NOT NULL,
                    acquired_at TEXT NOT NULL,
                    heartbeat_at TEXT NOT NULL,
                    expires_at TEXT NOT NULL
                );

                CREATE INDEX idx_tasks_status ON tasks(status);
                CREATE INDEX idx_task_labels_label ON task_labels(label, task_id);
                CREATE INDEX idx_task_dependencies_target ON task_dependencies(depends_on_id);
                CREATE INDEX idx_task_versions_task ON task_versions(task_id, version_no);
                CREATE INDEX idx_operation_history_task ON operation_history(task_id, event_id);

                CREATE TRIGGER prevent_task_delete
                BEFORE DELETE ON tasks
                BEGIN
                    SELECT RAISE(ABORT, 'physical task deletion is prohibited');
                END;

                CREATE TRIGGER immutable_task_versions_update
                BEFORE UPDATE ON task_versions
                BEGIN
                    SELECT RAISE(ABORT, 'task versions are immutable');
                END;

                CREATE TRIGGER immutable_task_versions_delete
                BEFORE DELETE ON task_versions
                BEGIN
                    SELECT RAISE(ABORT, 'task versions are immutable');
                END;

                CREATE TRIGGER immutable_operation_history_update
                BEFORE UPDATE ON operation_history
                BEGIN
                    SELECT RAISE(ABORT, 'operation history is immutable');
                END;

                CREATE TRIGGER immutable_operation_history_delete
                BEFORE DELETE ON operation_history
                BEGIN
                    SELECT RAISE(ABORT, 'operation history is immutable');
                END;

                INSERT INTO project_meta(key, value) VALUES ('schema_version', '1');
                ",
            )
            .map_err(|error| format!("Cannot initialize Todoing database schema: {error}"))?;

        transaction
            .pragma_update(None, "user_version", SCHEMA_VERSION)
            .map_err(|error| error.to_string())?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn create_task(&mut self, new_task: NewTask) -> Result<StoredTask, String> {
        validate_new_task(&new_task)?;
        self.config.validate_new_status(&new_task.status)?;
        let outcome = self
            .config
            .outcome_for(&new_task.status)
            .map(|outcome| outcome.as_str().to_string());
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let now = now_string();
        let metadata =
            serde_json::to_string(&new_task.metadata).map_err(|error| error.to_string())?;

        transaction
            .execute(
                "INSERT INTO tasks(title, status, outcome, body, created_at, updated_at, metadata_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6)",
                params![
                    new_task.title,
                    new_task.status,
                    outcome,
                    new_task.body,
                    now,
                    metadata
                ],
            )
            .map_err(|error| error.to_string())?;
        let id = transaction.last_insert_rowid();

        replace_labels(&transaction, id, &new_task.labels)?;
        replace_commits(&transaction, id, &new_task.commits)?;

        let task = load_task(&transaction, id)?
            .ok_or_else(|| "Created task could not be read back".to_string())?;
        append_task_change(&transaction, &task, None, &new_task.actor, "create")?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(task)
    }

    pub fn get_task(&mut self, id: i64) -> Result<Option<StoredTask>, String> {
        let transaction = self
            .connection
            .transaction()
            .map_err(|error| error.to_string())?;
        let task = load_task(&transaction, id)?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(task)
    }

    pub fn list_tasks(&mut self) -> Result<Vec<StoredTask>, String> {
        let transaction = self
            .connection
            .transaction()
            .map_err(|error| error.to_string())?;
        let ids = {
            let mut statement = transaction
                .prepare("SELECT id FROM tasks ORDER BY id")
                .map_err(|error| error.to_string())?;
            statement
                .query_map([], |row| row.get::<_, i64>(0))
                .map_err(|error| error.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| error.to_string())?
        };
        let tasks = ids
            .into_iter()
            .map(|id| {
                load_task(&transaction, id)?
                    .ok_or_else(|| format!("Task #{id} disappeared while listing"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(tasks)
    }

    pub fn update_task(&mut self, task: &StoredTask, actor: &str) -> Result<StoredTask, String> {
        validate_task(task, actor)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let before = load_task(&transaction, task.id)?
            .ok_or_else(|| format!("Task #{} not found", task.id))?;

        let mut candidate = task.clone();
        candidate.created_at = before.created_at.clone();
        candidate.updated_at = before.updated_at.clone();
        candidate.dependencies = before.dependencies.clone();
        if candidate.status != before.status {
            self.config
                .validate_transition(&before.status, &candidate.status)?;
            candidate.outcome = self
                .config
                .outcome_for(&candidate.status)
                .map(|outcome| outcome.as_str().to_string());
        } else {
            candidate.outcome = before.outcome.clone();
        }

        if candidate == before {
            return Ok(before);
        }

        candidate.updated_at = now_string();
        let metadata =
            serde_json::to_string(&candidate.metadata).map_err(|error| error.to_string())?;
        transaction
            .execute(
                "UPDATE tasks
                 SET title = ?1, status = ?2, outcome = ?3, body = ?4,
                     updated_at = ?5, metadata_json = ?6
                 WHERE id = ?7",
                params![
                    candidate.title,
                    candidate.status,
                    candidate.outcome,
                    candidate.body,
                    candidate.updated_at,
                    metadata,
                    candidate.id
                ],
            )
            .map_err(|error| error.to_string())?;
        replace_labels(&transaction, candidate.id, &candidate.labels)?;
        replace_commits(&transaction, candidate.id, &candidate.commits)?;

        let after = load_task(&transaction, candidate.id)?
            .ok_or_else(|| format!("Task #{} disappeared during update", candidate.id))?;
        append_task_change(&transaction, &after, Some(&before), actor, "update")?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(after)
    }

    pub fn add_dependency(
        &mut self,
        task_id: i64,
        depends_on_id: i64,
        actor: &str,
    ) -> Result<StoredTask, String> {
        if task_id == depends_on_id {
            return Err("A task cannot depend on itself".into());
        }

        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let before = load_task(&transaction, task_id)?
            .ok_or_else(|| format!("Task #{task_id} not found"))?;
        if load_task(&transaction, depends_on_id)?.is_none() {
            return Err(format!("Dependency task #{depends_on_id} not found"));
        }

        let creates_cycle: bool = transaction
            .query_row(
                "WITH RECURSIVE reachable(id) AS (
                    SELECT depends_on_id FROM task_dependencies WHERE task_id = ?1
                    UNION
                    SELECT dependency.depends_on_id
                    FROM task_dependencies AS dependency
                    JOIN reachable ON dependency.task_id = reachable.id
                 )
                 SELECT EXISTS(SELECT 1 FROM reachable WHERE id = ?2)",
                params![depends_on_id, task_id],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if creates_cycle {
            return Err(format!(
                "Adding dependency #{depends_on_id} to task #{task_id} would create a cycle"
            ));
        }

        let inserted = transaction
            .execute(
                "INSERT OR IGNORE INTO task_dependencies(task_id, depends_on_id, created_at)
                 VALUES (?1, ?2, ?3)",
                params![task_id, depends_on_id, now_string()],
            )
            .map_err(|error| error.to_string())?;
        if inserted == 0 {
            return Ok(before);
        }

        transaction
            .execute(
                "UPDATE tasks SET updated_at = ?1 WHERE id = ?2",
                params![now_string(), task_id],
            )
            .map_err(|error| error.to_string())?;
        let after = load_task(&transaction, task_id)?
            .ok_or_else(|| format!("Task #{task_id} disappeared during dependency update"))?;
        append_task_change(&transaction, &after, Some(&before), actor, "dependency_add")?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(after)
    }

    pub fn remove_dependency(
        &mut self,
        task_id: i64,
        depends_on_id: i64,
        actor: &str,
    ) -> Result<StoredTask, String> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let before = load_task(&transaction, task_id)?
            .ok_or_else(|| format!("Task #{task_id} not found"))?;
        let removed = transaction
            .execute(
                "DELETE FROM task_dependencies WHERE task_id = ?1 AND depends_on_id = ?2",
                params![task_id, depends_on_id],
            )
            .map_err(|error| error.to_string())?;
        if removed == 0 {
            return Ok(before);
        }

        transaction
            .execute(
                "UPDATE tasks SET updated_at = ?1 WHERE id = ?2",
                params![now_string(), task_id],
            )
            .map_err(|error| error.to_string())?;
        let after = load_task(&transaction, task_id)?
            .ok_or_else(|| format!("Task #{task_id} disappeared during dependency update"))?;
        append_task_change(
            &transaction,
            &after,
            Some(&before),
            actor,
            "dependency_remove",
        )?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(after)
    }

    pub fn get_versions(&self, task_id: i64) -> Result<Vec<TaskVersion>, String> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT version_no, snapshot_json, created_at, actor
                 FROM task_versions WHERE task_id = ?1 ORDER BY version_no",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map([task_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .map_err(|error| error.to_string())?;

        rows.map(|row| {
            let (version_no, snapshot_json, created_at, actor) =
                row.map_err(|error| error.to_string())?;
            let snapshot = serde_json::from_str(&snapshot_json)
                .map_err(|error| format!("Invalid task version snapshot: {error}"))?;
            Ok(TaskVersion {
                version_no,
                snapshot,
                created_at,
                actor,
            })
        })
        .collect()
    }

    pub fn get_history(&self, task_id: i64) -> Result<Vec<HistoryEntry>, String> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT event_id, operation, actor, occurred_at, before_json, after_json, result
                 FROM operation_history WHERE task_id = ?1 ORDER BY event_id",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map([task_id], |row| {
                Ok(HistoryEntry {
                    event_id: row.get(0)?,
                    operation: row.get(1)?,
                    actor: row.get(2)?,
                    occurred_at: row.get(3)?,
                    before_json: row.get(4)?,
                    after_json: row.get(5)?,
                    result: row.get(6)?,
                })
            })
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())
    }

    pub fn approve_version(
        &mut self,
        task_id: i64,
        version_no: i64,
        actor: &str,
    ) -> Result<(), String> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let version_id: i64 = transaction
            .query_row(
                "SELECT version_id FROM task_versions WHERE task_id = ?1 AND version_no = ?2",
                params![task_id, version_no],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("Version {version_no} for task #{task_id} not found"))?;

        let before: Option<(i64, String, String)> = transaction
            .query_row(
                "SELECT version_id, approved_by, approved_at
                 FROM task_version_approvals WHERE task_id = ?1",
                [task_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        let before_json = before.as_ref().map(|(id, approved_by, approved_at)| {
            serde_json::json!({
                "version_id": id,
                "approved_by": approved_by,
                "approved_at": approved_at
            })
            .to_string()
        });

        let approved_at = now_string();
        transaction
            .execute(
                "INSERT INTO task_version_approvals(task_id, version_id, approved_by, approved_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(task_id) DO UPDATE SET
                    version_id = excluded.version_id,
                    approved_by = excluded.approved_by,
                    approved_at = excluded.approved_at",
                params![task_id, version_id, actor, approved_at],
            )
            .map_err(|error| error.to_string())?;
        let after_json = serde_json::json!({
            "version_id": version_id,
            "approved_by": actor,
            "approved_at": approved_at
        })
        .to_string();
        append_history(
            &transaction,
            task_id,
            actor,
            "baseline_approve",
            before_json.as_deref(),
            Some(&after_json),
        )?;
        transaction.commit().map_err(|error| error.to_string())
    }

    pub fn get_approved_version(&self, task_id: i64) -> Result<Option<TaskVersion>, String> {
        let row: Option<(i64, String, String, String)> = self
            .connection
            .query_row(
                "SELECT versions.version_no, versions.snapshot_json, versions.created_at, versions.actor
                 FROM task_version_approvals AS approvals
                 JOIN task_versions AS versions
                   ON versions.task_id = approvals.task_id
                  AND versions.version_id = approvals.version_id
                 WHERE approvals.task_id = ?1",
                [task_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        row.map(|(version_no, snapshot_json, created_at, actor)| {
            let snapshot = serde_json::from_str(&snapshot_json)
                .map_err(|error| format!("Invalid task version snapshot: {error}"))?;
            Ok(TaskVersion {
                version_no,
                snapshot,
                created_at,
                actor,
            })
        })
        .transpose()
    }

    pub fn counts(&mut self) -> Result<StoreCounts, String> {
        let transaction = self
            .connection
            .transaction()
            .map_err(|error| error.to_string())?;
        let count = |table: &str| -> Result<u64, String> {
            let sql = format!("SELECT COUNT(*) FROM {table}");
            transaction
                .query_row(&sql, [], |row| row.get::<_, i64>(0))
                .map(|value| value as u64)
                .map_err(|error| error.to_string())
        };
        let counts = StoreCounts {
            tasks: count("tasks")?,
            versions: count("task_versions")?,
            history: count("operation_history")?,
        };
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(counts)
    }

    pub fn integrity_check(&self) -> Result<(), String> {
        let integrity: String = self
            .connection
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .map_err(|error| error.to_string())?;
        if integrity != "ok" {
            return Err(format!("SQLite integrity check failed: {integrity}"));
        }
        let foreign_key_violations: i64 = self
            .connection
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })
            .map_err(|error| error.to_string())?;
        if foreign_key_violations != 0 {
            return Err(format!(
                "SQLite foreign key check found {foreign_key_violations} violation(s)"
            ));
        }
        Ok(())
    }
}

fn validate_new_task(task: &NewTask) -> Result<(), String> {
    if task.title.trim().is_empty() {
        return Err("Task title cannot be empty".into());
    }
    if task.status.trim().is_empty() {
        return Err("Task status cannot be empty".into());
    }
    validate_actor(&task.actor)?;
    validate_labels(&task.labels)?;
    validate_commits(&task.commits)?;
    Ok(())
}

fn validate_task(task: &StoredTask, actor: &str) -> Result<(), String> {
    if task.title.trim().is_empty() {
        return Err("Task title cannot be empty".into());
    }
    if task.status.trim().is_empty() {
        return Err("Task status cannot be empty".into());
    }
    validate_outcome(task.outcome.as_deref())?;
    validate_actor(actor)?;
    validate_labels(&task.labels)?;
    validate_commits(&task.commits)?;
    Ok(())
}

fn validate_outcome(outcome: Option<&str>) -> Result<(), String> {
    match outcome {
        None | Some("completed") | Some("cancelled") => Ok(()),
        Some(value) => Err(format!("Invalid task outcome '{value}'")),
    }
}

fn validate_actor(actor: &str) -> Result<(), String> {
    if actor.trim().is_empty() {
        return Err("Actor identifier cannot be empty".into());
    }
    Ok(())
}

fn validate_labels(labels: &[String]) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    for label in labels {
        if label.trim().is_empty() {
            return Err("Task labels cannot be empty".into());
        }
        if !seen.insert(label) {
            return Err(format!("Duplicate task label '{label}'"));
        }
    }
    Ok(())
}

fn validate_commits(commits: &[String]) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    for sha in commits {
        if !(sha.len() == 40 || sha.len() == 64)
            || !sha.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(format!(
                "Git commit reference must be a full 40- or 64-character hexadecimal SHA: '{sha}'"
            ));
        }
        if !seen.insert(sha) {
            return Err(format!("Duplicate Git commit reference '{sha}'"));
        }
    }
    Ok(())
}

fn now_string() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Micros, true)
}

fn replace_labels(
    transaction: &Transaction<'_>,
    task_id: i64,
    labels: &[String],
) -> Result<(), String> {
    transaction
        .execute("DELETE FROM task_labels WHERE task_id = ?1", [task_id])
        .map_err(|error| error.to_string())?;
    for (position, label) in labels.iter().enumerate() {
        transaction
            .execute(
                "INSERT INTO task_labels(task_id, label, position) VALUES (?1, ?2, ?3)",
                params![task_id, label, position as i64],
            )
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn replace_commits(
    transaction: &Transaction<'_>,
    task_id: i64,
    commits: &[String],
) -> Result<(), String> {
    transaction
        .execute("DELETE FROM task_git_commits WHERE task_id = ?1", [task_id])
        .map_err(|error| error.to_string())?;
    for (position, sha) in commits.iter().enumerate() {
        transaction
            .execute(
                "INSERT INTO task_git_commits(task_id, sha, position) VALUES (?1, ?2, ?3)",
                params![task_id, sha, position as i64],
            )
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn load_task(connection: &Connection, id: i64) -> Result<Option<StoredTask>, String> {
    let row: Option<(
        i64,
        String,
        String,
        Option<String>,
        String,
        String,
        String,
        String,
    )> = connection
        .query_row(
            "SELECT id, title, status, outcome, body, created_at, updated_at, metadata_json
             FROM tasks WHERE id = ?1",
            [id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                ))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let Some((id, title, status, outcome, body, created_at, updated_at, metadata_json)) = row
    else {
        return Ok(None);
    };

    let labels = connection
        .prepare("SELECT label FROM task_labels WHERE task_id = ?1 ORDER BY position")
        .and_then(|mut statement| {
            statement
                .query_map([id], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()
        })
        .map_err(|error| error.to_string())?;
    let commits = connection
        .prepare("SELECT sha FROM task_git_commits WHERE task_id = ?1 ORDER BY position")
        .and_then(|mut statement| {
            statement
                .query_map([id], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()
        })
        .map_err(|error| error.to_string())?;
    let dependencies = connection
        .prepare(
            "SELECT depends_on_id FROM task_dependencies WHERE task_id = ?1 ORDER BY depends_on_id",
        )
        .and_then(|mut statement| {
            statement
                .query_map([id], |row| row.get::<_, i64>(0))?
                .collect::<Result<Vec<_>, _>>()
        })
        .map_err(|error| error.to_string())?;
    let metadata = serde_json::from_str(&metadata_json)
        .map_err(|error| format!("Invalid task metadata JSON for #{id}: {error}"))?;

    Ok(Some(StoredTask {
        id,
        title,
        status,
        outcome,
        body,
        created_at,
        updated_at,
        labels,
        commits,
        dependencies,
        metadata,
    }))
}

fn append_task_change(
    transaction: &Transaction<'_>,
    after: &StoredTask,
    before: Option<&StoredTask>,
    actor: &str,
    operation: &str,
) -> Result<(), String> {
    let version_no: i64 = transaction
        .query_row(
            "SELECT COALESCE(MAX(version_no), 0) + 1
             FROM task_versions WHERE task_id = ?1",
            [after.id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let snapshot_json = serde_json::to_string(after).map_err(|error| error.to_string())?;
    let created_at = now_string();
    transaction
        .execute(
            "INSERT INTO task_versions(task_id, version_no, snapshot_json, created_at, actor)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![after.id, version_no, snapshot_json, created_at, actor],
        )
        .map_err(|error| error.to_string())?;
    let before_json = before
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| error.to_string())?;
    append_history(
        transaction,
        after.id,
        actor,
        operation,
        before_json.as_deref(),
        Some(&snapshot_json),
    )
}

fn append_history(
    transaction: &Transaction<'_>,
    task_id: i64,
    actor: &str,
    operation: &str,
    before_json: Option<&str>,
    after_json: Option<&str>,
) -> Result<(), String> {
    validate_actor(actor)?;
    transaction
        .execute(
            "INSERT INTO operation_history(task_id, actor, occurred_at, operation, before_json, after_json, result)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'success')",
            params![task_id, actor, now_string(), operation, before_json, after_json],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}
