use super::{Store, StoreError};
use crate::domain::{StatusId, TaskId, TaskOutcome};
use chrono::{SecondsFormat, Utc};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewTask {
    pub title: String,
    pub status_id: StatusId,
    pub outcome: Option<TaskOutcome>,
    pub body: String,
    pub labels: Vec<String>,
    pub commit_refs: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct TaskRepository {
    store: Store,
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

    pub fn create(&self, new_task: NewTask) -> Result<Task, StoreError> {
        validate_task_input(
            &new_task.title,
            &new_task.labels,
            &new_task.commit_refs,
        )?;

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
        transaction.commit()?;

        self.get(id)
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

    pub fn update(&self, task: &Task) -> Result<Task, StoreError> {
        validate_task_input(&task.title, &task.labels, &task.commit_refs)?;

        let mut connection = self.store.connection()?;
        let transaction =
            connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let now = now_string();

        let changed = transaction.execute(
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
        if changed == 0 {
            return Err(StoreError::TaskNotFound(task.id.get()));
        }

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
        transaction.commit()?;

        self.get(task.id)
    }

    pub fn store(&self) -> &Store {
        &self.store
    }
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
                "Invalid Git commit SHA '{}'",
                sha
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

fn now_string() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Micros, true)
}
