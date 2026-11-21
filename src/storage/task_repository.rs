use super::{Store, StoreError};
use crate::domain::{ActorContext, ActorId, StatusId, TaskId, TaskOutcome};
use chrono::{SecondsFormat, Utc};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashSet, path::Path};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub title: String,
    pub status_id: StatusId,
    pub outcome: Option<TaskOutcome>,
    pub body: String,
    pub created_at: String,
    pub updated_at: String,
    pub labels: Vec<String>,
    pub commit_refs: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewTask {
    pub title: String,
    pub status_id: StatusId,
    pub outcome: Option<TaskOutcome>,
    pub body: String,
    pub labels: Vec<String>,
    pub commit_refs: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryOutcome {
    Succeeded,
    Rejected,
}

impl HistoryOutcome {
    fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Rejected => "rejected",
        }
    }

    fn parse(value: &str) -> Result<Self, StoreError> {
        match value {
            "succeeded" => Ok(Self::Succeeded),
            "rejected" => Ok(Self::Rejected),
            other => Err(StoreError::InvalidTask(format!(
                "Unknown operation history outcome '{other}'"
            ))),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskVersion {
    pub version_id: i64,
    pub task_id: TaskId,
    pub version_no: i64,
    pub snapshot_json: String,
    pub created_at: String,
    pub actor_id: ActorId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub event_id: i64,
    pub task_id: Option<TaskId>,
    pub actor_id: ActorId,
    pub occurred_at: String,
    pub operation: String,
    pub previous_json: Option<String>,
    pub requested_json: Option<String>,
    pub outcome: HistoryOutcome,
    pub error_code: Option<String>,
    pub details_json: Option<String>,
    pub config_revision: Option<String>,
    pub fencing_generation: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskBaseline {
    pub task_id: TaskId,
    pub version_id: i64,
    pub version_no: i64,
    pub approved_at: String,
    pub approved_by: ActorId,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TaskFieldChange {
    pub field: String,
    pub before: Value,
    pub after: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TaskVersionDiff {
    pub task_id: TaskId,
    pub from_version: i64,
    pub to_version: i64,
    pub changes: Vec<TaskFieldChange>,
}

#[derive(Clone, Debug)]
pub struct TaskRepository {
    store: Store,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct TaskSnapshot {
    task_id: TaskId,
    title: String,
    status_id: StatusId,
    outcome: Option<TaskOutcome>,
    body: String,
    created_at: String,
    updated_at: String,
    labels: Vec<String>,
    commit_refs: Vec<String>,
    dependencies: Vec<TaskId>,
}

impl TaskRepository {
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    pub fn open(
        root: impl AsRef<Path>,
        expected_project_id: Option<&crate::domain::ProjectId>,
    ) -> Result<Self, StoreError> {
        Ok(Self::new(Store::open(root, expected_project_id)?))
    }

    pub fn create(
        &self,
        new_task: NewTask,
        actor: &ActorContext,
    ) -> Result<Task, StoreError> {
        if let Err(error) =
            validate_task_input(&new_task.title, &new_task.labels, &new_task.commit_refs)
        {
            let requested = serde_json::to_string(&new_task).ok();
            self.record_rejection(
                None,
                actor,
                "task.create",
                "invalid_request",
                requested.as_deref(),
            )?;
            return Err(error);
        }

        let mut connection = self.store.connection()?;
        let transaction =
            connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let now = now_string();

        transaction.execute(
            "INSERT INTO tasks(title, status_id, outcome, body, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
            params![
                new_task.title.trim(),
                new_task.status_id.as_str(),
                outcome_as_str(new_task.outcome),
                new_task.body,
                now
            ],
        )?;
        let id = TaskId::new(transaction.last_insert_rowid())
            .map_err(|error| StoreError::InvalidTask(error.to_string()))?;

        insert_labels(&transaction, id, &new_task.labels)?;
        insert_commit_refs(&transaction, id, &new_task.commit_refs)?;
        let task = load_task(&transaction, id)?;
        let snapshot = snapshot_json(&transaction, &task)?;
        insert_version(&transaction, id, 1, &snapshot, &now, actor.actor_id().as_str())?;
        insert_history(
            &transaction,
            Some(id),
            actor.actor_id().as_str(),
            &now,
            "task.create",
            None,
            Some(&snapshot),
            HistoryOutcome::Succeeded,
            None,
            None,
            None,
        )?;
        transaction.commit()?;

        Ok(task)
    }

    pub fn get(&self, id: TaskId) -> Result<Task, StoreError> {
        let mut connection = self.store.connection()?;
        let transaction = connection.transaction()?;
        let task = load_task(&transaction, id)?;
        transaction.commit()?;
        Ok(task)
    }

    pub fn list(&self) -> Result<Vec<Task>, StoreError> {
        let mut connection = self.store.connection()?;
        let transaction = connection.transaction()?;
        let ids = {
            let mut statement =
                transaction.prepare("SELECT task_id FROM tasks ORDER BY task_id ASC")?;
            let rows = statement.query_map([], |row| row.get::<_, i64>(0))?;
            rows.collect::<Result<Vec<_>, _>>()?
        };

        let mut tasks = Vec::with_capacity(ids.len());
        for raw_id in ids {
            let id = TaskId::new(raw_id)
                .map_err(|error| StoreError::InvalidTask(error.to_string()))?;
            tasks.push(load_task(&transaction, id)?);
        }
        transaction.commit()?;
        Ok(tasks)
    }

    pub fn update(
        &self,
        task: &Task,
        actor: &ActorContext,
    ) -> Result<Task, StoreError> {
        if let Err(error) = validate_task_input(&task.title, &task.labels, &task.commit_refs) {
            let requested = serde_json::to_string(task).ok();
            self.record_rejection(
                Some(task.id),
                actor,
                "task.update",
                "invalid_request",
                requested.as_deref(),
            )?;
            return Err(error);
        }

        let mut connection = self.store.connection()?;
        let transaction =
            connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let previous = match load_task(&transaction, task.id) {
            Ok(previous) => previous,
            Err(error @ StoreError::TaskNotFound(_)) => {
                drop(transaction);
                let requested = serde_json::to_string(task).ok();
                self.record_rejection(
                    Some(task.id),
                    actor,
                    "task.update",
                    "task_not_found",
                    requested.as_deref(),
                )?;
                return Err(error);
            }
            Err(error) => return Err(error),
        };

        if same_task_state(&previous, task) {
            drop(transaction);
            return Ok(previous);
        }

        let now = now_string();
        transaction.execute(
            "UPDATE tasks
             SET title = ?1, status_id = ?2, outcome = ?3, body = ?4, updated_at = ?5
             WHERE task_id = ?6",
            params![
                task.title.trim(),
                task.status_id.as_str(),
                outcome_as_str(task.outcome),
                task.body,
                now,
                task.id.get()
            ],
        )?;
        transaction.execute(
            "DELETE FROM task_labels WHERE task_id = ?1",
            params![task.id.get()],
        )?;
        transaction.execute(
            "DELETE FROM task_commit_refs WHERE task_id = ?1",
            params![task.id.get()],
        )?;
        insert_labels(&transaction, task.id, &task.labels)?;
        insert_commit_refs(&transaction, task.id, &task.commit_refs)?;

        let updated = load_task(&transaction, task.id)?;
        let previous_snapshot = snapshot_json(&transaction, &previous)?;
        let updated_snapshot = snapshot_json(&transaction, &updated)?;
        let version_no = next_version_no(&transaction, task.id)?;
        insert_version(
            &transaction,
            task.id,
            version_no,
            &updated_snapshot,
            &now,
            actor.actor_id().as_str(),
        )?;
        insert_history(
            &transaction,
            Some(task.id),
            actor.actor_id().as_str(),
            &now,
            "task.update",
            Some(&previous_snapshot),
            Some(&updated_snapshot),
            HistoryOutcome::Succeeded,
            None,
            None,
            None,
        )?;
        transaction.commit()?;

        Ok(updated)
    }

    pub fn list_versions(&self, task_id: TaskId) -> Result<Vec<TaskVersion>, StoreError> {
        let connection = self.store.connection()?;
        let mut statement = connection.prepare(
            "SELECT version_id, task_id, version_no, snapshot_json, created_at, actor_id
             FROM task_versions WHERE task_id = ?1 ORDER BY version_no ASC",
        )?;
        let rows = statement.query_map(params![task_id.get()], map_version)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(StoreError::from)
    }

    pub fn get_version(
        &self,
        task_id: TaskId,
        version_no: i64,
    ) -> Result<TaskVersion, StoreError> {
        let connection = self.store.connection()?;
        connection
            .query_row(
                "SELECT version_id, task_id, version_no, snapshot_json, created_at, actor_id
                 FROM task_versions WHERE task_id = ?1 AND version_no = ?2",
                params![task_id.get(), version_no],
                map_version,
            )
            .optional()?
            .ok_or(StoreError::VersionNotFound {
                task_id: task_id.get(),
                version_no,
            })
    }

    pub fn get_history(&self, task_id: TaskId) -> Result<Vec<HistoryEntry>, StoreError> {
        let connection = self.store.connection()?;
        let mut statement = connection.prepare(
            "SELECT event_id, task_id, actor_id, occurred_at, operation,
                    previous_json, requested_json, outcome, error_code,
                    details_json, config_revision, fencing_generation
             FROM operation_history WHERE task_id = ?1 ORDER BY event_id ASC",
        )?;
        let rows = statement.query_map(params![task_id.get()], map_history)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(StoreError::from)
    }

    pub fn get_store_history(&self) -> Result<Vec<HistoryEntry>, StoreError> {
        let connection = self.store.connection()?;
        let mut statement = connection.prepare(
            "SELECT event_id, task_id, actor_id, occurred_at, operation,
                    previous_json, requested_json, outcome, error_code,
                    details_json, config_revision, fencing_generation
             FROM operation_history ORDER BY event_id ASC",
        )?;
        let rows = statement.query_map([], map_history)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(StoreError::from)
    }

    pub fn approve_baseline(
        &self,
        task_id: TaskId,
        version_no: i64,
        actor: &ActorContext,
    ) -> Result<TaskBaseline, StoreError> {
        let mut connection = self.store.connection()?;
        let transaction =
            connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let version_id = transaction
            .query_row(
                "SELECT version_id FROM task_versions WHERE task_id = ?1 AND version_no = ?2",
                params![task_id.get(), version_no],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        let Some(version_id) = version_id else {
            drop(transaction);
            self.record_rejection(
                Some(task_id),
                actor,
                "task.version.approve",
                "version_not_found",
                Some(&format!(r#"{{"version_no":{version_no}}}"#)),
            )?;
            return Err(StoreError::VersionNotFound {
                task_id: task_id.get(),
                version_no,
            });
        };

        let approved_at = now_string();
        transaction.execute(
            "INSERT INTO task_baselines(task_id, version_id, approved_at, approved_by)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(task_id) DO UPDATE SET
                 version_id = excluded.version_id,
                 approved_at = excluded.approved_at,
                 approved_by = excluded.approved_by",
            params![
                task_id.get(),
                version_id,
                approved_at,
                actor.actor_id().as_str()
            ],
        )?;
        insert_history(
            &transaction,
            Some(task_id),
            actor.actor_id().as_str(),
            &approved_at,
            "task.version.approve",
            None,
            Some(&format!(r#"{{"version_no":{version_no}}}"#)),
            HistoryOutcome::Succeeded,
            None,
            None,
            None,
        )?;
        let baseline = TaskBaseline {
            task_id,
            version_id,
            version_no,
            approved_at,
            approved_by: actor.actor_id().clone(),
        };
        transaction.commit()?;
        Ok(baseline)
    }

    pub fn get_baseline(&self, task_id: TaskId) -> Result<Option<TaskBaseline>, StoreError> {
        let connection = self.store.connection()?;
        connection
            .query_row(
                "SELECT b.task_id, b.version_id, v.version_no, b.approved_at, b.approved_by
                 FROM task_baselines b
                 JOIN task_versions v
                   ON v.task_id = b.task_id AND v.version_id = b.version_id
                 WHERE b.task_id = ?1",
                params![task_id.get()],
                |row| {
                    let raw_task_id: i64 = row.get(0)?;
                    let approved_by: String = row.get(4)?;
                    Ok((raw_task_id, row.get(1)?, row.get(2)?, row.get(3)?, approved_by))
                },
            )
            .optional()?
            .map(|(raw_task_id, version_id, version_no, approved_at, approved_by)| {
                Ok(TaskBaseline {
                    task_id: TaskId::new(raw_task_id)
                        .map_err(|error| StoreError::InvalidTask(error.to_string()))?,
                    version_id,
                    version_no,
                    approved_at,
                    approved_by: ActorId::new(approved_by)
                        .map_err(|error| StoreError::InvalidTask(error.to_string()))?,
                })
            })
            .transpose()
    }

    pub fn compare_versions(
        &self,
        task_id: TaskId,
        from_version: i64,
        to_version: i64,
    ) -> Result<TaskVersionDiff, StoreError> {
        let before = self.load_snapshot(task_id, from_version)?;
        let after = self.load_snapshot(task_id, to_version)?;
        let mut changes = Vec::new();

        push_change(&mut changes, "title", &before.title, &after.title)?;
        push_change(&mut changes, "status_id", &before.status_id, &after.status_id)?;
        push_change(&mut changes, "outcome", &before.outcome, &after.outcome)?;
        push_change(&mut changes, "body", &before.body, &after.body)?;
        push_change(&mut changes, "labels", &before.labels, &after.labels)?;
        push_change(&mut changes, "commit_refs", &before.commit_refs, &after.commit_refs)?;
        push_change(&mut changes, "dependencies", &before.dependencies, &after.dependencies)?;

        Ok(TaskVersionDiff {
            task_id,
            from_version,
            to_version,
            changes,
        })
    }

    pub fn record_rejection(
        &self,
        task_id: Option<TaskId>,
        actor: &ActorContext,
        operation: &str,
        error_code: &str,
        requested_json: Option<&str>,
    ) -> Result<(), StoreError> {
        let mut connection = self.store.connection()?;
        let transaction =
            connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let now = now_string();
        insert_history(
            &transaction,
            task_id,
            actor.actor_id().as_str(),
            &now,
            operation,
            None,
            requested_json,
            HistoryOutcome::Rejected,
            Some(error_code),
            None,
            None,
        )
        .map_err(|error| StoreError::HistoryRecordFailed(error.to_string()))?;
        transaction
            .commit()
            .map_err(|error| StoreError::HistoryRecordFailed(error.to_string()))
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    fn load_snapshot(
        &self,
        task_id: TaskId,
        version_no: i64,
    ) -> Result<TaskSnapshot, StoreError> {
        let version = self.get_version(task_id, version_no)?;
        let snapshot: TaskSnapshot = serde_json::from_str(&version.snapshot_json)
            .map_err(|error| StoreError::InvalidTask(format!("Invalid task version snapshot: {error}")))?;
        if snapshot.task_id != task_id {
            return Err(StoreError::InvalidTask(
                "Task version snapshot identity mismatch".into(),
            ));
        }
        Ok(snapshot)
    }
}

fn same_task_state(current: &Task, requested: &Task) -> bool {
    current.title == requested.title.trim()
        && current.status_id == requested.status_id
        && current.outcome == requested.outcome
        && current.body == requested.body
        && current.labels == requested.labels.iter().map(|label| label.trim().to_owned()).collect::<Vec<_>>()
        && current.commit_refs == requested.commit_refs
}

fn load_task(connection: &Connection, id: TaskId) -> Result<Task, StoreError> {
    let row = connection
        .query_row(
            "SELECT title, status_id, outcome, body, created_at, updated_at
             FROM tasks WHERE task_id = ?1",
            params![id.get()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )
        .optional()?
        .ok_or(StoreError::TaskNotFound(id.get()))?;

    let labels = {
        let mut statement = connection.prepare(
            "SELECT label FROM task_labels WHERE task_id = ?1 ORDER BY position ASC",
        )?;
        let rows = statement.query_map(params![id.get()], |row| row.get(0))?;
        rows.collect::<Result<Vec<String>, _>>()?
    };
    let commit_refs = {
        let mut statement = connection.prepare(
            "SELECT sha FROM task_commit_refs WHERE task_id = ?1 ORDER BY position ASC",
        )?;
        let rows = statement.query_map(params![id.get()], |row| row.get(0))?;
        rows.collect::<Result<Vec<String>, _>>()?
    };

    let outcome = match row.2.as_deref() {
        None => None,
        Some("completed") => Some(TaskOutcome::Completed),
        Some("cancelled") => Some(TaskOutcome::Cancelled),
        Some(value) => {
            return Err(StoreError::InvalidTask(format!(
                "Task #{} has unknown outcome '{}'",
                id.get(),
                value
            )));
        }
    };

    Ok(Task {
        id,
        title: row.0,
        status_id: StatusId::new(row.1)
            .map_err(|error| StoreError::InvalidTask(error.to_string()))?,
        outcome,
        body: row.3,
        created_at: row.4,
        updated_at: row.5,
        labels,
        commit_refs,
    })
}

fn snapshot_json(connection: &Connection, task: &Task) -> Result<String, StoreError> {
    let dependencies = {
        let mut statement = connection.prepare(
            "SELECT depends_on_task_id FROM task_dependencies
             WHERE task_id = ?1 ORDER BY depends_on_task_id ASC",
        )?;
        let rows = statement.query_map(params![task.id.get()], |row| row.get::<_, i64>(0))?;
        rows.map(|row| {
            let raw_id = row?;
            TaskId::new(raw_id).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Integer,
                    Box::new(error),
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?
    };

    let snapshot = TaskSnapshot {
        task_id: task.id,
        title: task.title.clone(),
        status_id: task.status_id.clone(),
        outcome: task.outcome,
        body: task.body.clone(),
        created_at: task.created_at.clone(),
        updated_at: task.updated_at.clone(),
        labels: task.labels.clone(),
        commit_refs: task.commit_refs.clone(),
        dependencies,
    };
    serde_json::to_string(&snapshot)
        .map_err(|error| StoreError::InvalidTask(format!("Cannot serialize task snapshot: {error}")))
}

fn insert_version(
    transaction: &Transaction<'_>,
    task_id: TaskId,
    version_no: i64,
    snapshot_json: &str,
    created_at: &str,
    actor_id: &str,
) -> Result<(), StoreError> {
    transaction.execute(
        "INSERT INTO task_versions(task_id, version_no, snapshot_json, created_at, actor_id)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![task_id.get(), version_no, snapshot_json, created_at, actor_id],
    )?;
    Ok(())
}

fn next_version_no(transaction: &Transaction<'_>, task_id: TaskId) -> Result<i64, StoreError> {
    let current: i64 = transaction.query_row(
        "SELECT COALESCE(MAX(version_no), 0) FROM task_versions WHERE task_id = ?1",
        params![task_id.get()],
        |row| row.get(0),
    )?;
    Ok(current + 1)
}

#[allow(clippy::too_many_arguments)]
fn insert_history(
    transaction: &Transaction<'_>,
    task_id: Option<TaskId>,
    actor_id: &str,
    occurred_at: &str,
    operation: &str,
    previous_json: Option<&str>,
    requested_json: Option<&str>,
    outcome: HistoryOutcome,
    error_code: Option<&str>,
    details_json: Option<&str>,
    config_revision: Option<&str>,
) -> Result<(), StoreError> {
    transaction.execute(
        "INSERT INTO operation_history(
            task_id, actor_id, occurred_at, operation, previous_json,
            requested_json, outcome, error_code, details_json, config_revision
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            task_id.map(TaskId::get),
            actor_id,
            occurred_at,
            operation,
            previous_json,
            requested_json,
            outcome.as_str(),
            error_code,
            details_json,
            config_revision
        ],
    )?;
    Ok(())
}

fn map_version(row: &rusqlite::Row<'_>) -> rusqlite::Result<TaskVersion> {
    let raw_task_id: i64 = row.get(1)?;
    let actor_id: String = row.get(5)?;
    TaskId::new(raw_task_id)
        .and_then(|task_id| {
            ActorId::new(actor_id).map(|actor_id| (task_id, actor_id))
        })
        .map(|(task_id, actor_id)| TaskVersion {
            version_id: row.get(0).unwrap_or_default(),
            task_id,
            version_no: row.get(2).unwrap_or_default(),
            snapshot_json: row.get(3).unwrap_or_default(),
            created_at: row.get(4).unwrap_or_default(),
            actor_id,
        })
        .map_err(|error| rusqlite::Error::FromSqlConversionFailure(
            1,
            rusqlite::types::Type::Integer,
            Box::new(error),
        ))
}

fn map_history(row: &rusqlite::Row<'_>) -> rusqlite::Result<HistoryEntry> {
    let raw_task_id: Option<i64> = row.get(1)?;
    let actor_raw: String = row.get(2)?;
    let outcome_raw: String = row.get(7)?;
    let task_id = raw_task_id.map(TaskId::new).transpose().map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            1,
            rusqlite::types::Type::Integer,
            Box::new(error),
        )
    })?;
    let actor_id = ActorId::new(actor_raw).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            2,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })?;
    let outcome = HistoryOutcome::parse(&outcome_raw).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            7,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })?;

    Ok(HistoryEntry {
        event_id: row.get(0)?,
        task_id,
        actor_id,
        occurred_at: row.get(3)?,
        operation: row.get(4)?,
        previous_json: row.get(5)?,
        requested_json: row.get(6)?,
        outcome,
        error_code: row.get(8)?,
        details_json: row.get(9)?,
        config_revision: row.get(10)?,
        fencing_generation: row.get(11)?,
    })
}

fn validate_task_input(
    title: &str,
    labels: &[String],
    commit_refs: &[String],
) -> Result<(), StoreError> {
    if title.trim().is_empty() {
        return Err(StoreError::InvalidTask(
            "Task title must contain a non-whitespace character".into(),
        ));
    }

    let mut label_keys = HashSet::new();
    for label in labels {
        if label.trim().is_empty() {
            return Err(StoreError::InvalidTask(
                "Task labels must contain a non-whitespace character".into(),
            ));
        }
        let key = label.trim().to_lowercase();
        if !label_keys.insert(key) {
            return Err(StoreError::InvalidTask(
                "Task labels must be unique after normalization".into(),
            ));
        }
    }

    let mut commits = HashSet::new();
    for sha in commit_refs {
        let valid_length = sha.len() == 40 || sha.len() == 64;
        if !valid_length
            || !sha
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(StoreError::InvalidTask(format!(
                "Invalid Git commit SHA '{sha}'"
            )));
        }
        if !commits.insert(sha) {
            return Err(StoreError::InvalidTask(
                "Git commit references must be unique".into(),
            ));
        }
    }

    Ok(())
}

fn insert_labels(
    connection: &Connection,
    task_id: TaskId,
    labels: &[String],
) -> Result<(), StoreError> {
    for (position, label) in labels.iter().enumerate() {
        connection.execute(
            "INSERT INTO task_labels(task_id, position, label, label_key)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                task_id.get(),
                position as i64,
                label.trim(),
                label.trim().to_lowercase()
            ],
        )?;
    }
    Ok(())
}

fn insert_commit_refs(
    connection: &Connection,
    task_id: TaskId,
    commit_refs: &[String],
) -> Result<(), StoreError> {
    for (position, sha) in commit_refs.iter().enumerate() {
        connection.execute(
            "INSERT INTO task_commit_refs(task_id, position, sha)
             VALUES (?1, ?2, ?3)",
            params![task_id.get(), position as i64, sha],
        )?;
    }
    Ok(())
}

fn outcome_as_str(outcome: Option<TaskOutcome>) -> Option<&'static str> {
    match outcome {
        None => None,
        Some(TaskOutcome::Completed) => Some("completed"),
        Some(TaskOutcome::Cancelled) => Some("cancelled"),
    }
}

fn push_change<T: Serialize + PartialEq>(
    changes: &mut Vec<TaskFieldChange>,
    field: &str,
    before: &T,
    after: &T,
) -> Result<(), StoreError> {
    if before != after {
        changes.push(TaskFieldChange {
            field: field.to_owned(),
            before: serde_json::to_value(before)
                .map_err(|error| StoreError::InvalidTask(error.to_string()))?,
            after: serde_json::to_value(after)
                .map_err(|error| StoreError::InvalidTask(error.to_string()))?,
        });
    }
    Ok(())
}

fn now_string() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Micros, true)
}
