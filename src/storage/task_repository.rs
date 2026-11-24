use super::{Store, StoreError};
use super::lock::{Clock, LockCredential, LockGrant, SystemClock, TaskLockService, check_lock_access};
use crate::config::ProjectConfig;
use sha2::{Digest, Sha256};
use crate::domain::{ActorContext, ActorId, StatusId, TaskId, TaskOutcome};
use chrono::{SecondsFormat, Utc};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
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
    pub archived_at: Option<String>,
    pub archived_by: Option<ActorId>,
    pub labels: Vec<String>,
    pub commit_refs: Vec<String>,
    pub dependencies: Vec<TaskId>,
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
pub struct TaskArchiveResult {
    pub task: Task,
    pub no_op: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyState {
    pub task_id: TaskId,
    pub status_id: Option<StatusId>,
    pub outcome: Option<TaskOutcome>,
    pub archived: Option<bool>,
    pub locked: bool,
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
    config: ProjectConfig,
    lock_credential: Option<LockCredential>,
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
    #[serde(default)]
    archived_at: Option<String>,
    #[serde(default)]
    archived_by: Option<ActorId>,
    labels: Vec<String>,
    commit_refs: Vec<String>,
    dependencies: Vec<TaskId>,
    #[serde(default)]
    revision_count: i64,
    #[serde(default)]
    final_commit_sha: Option<String>,
    #[serde(default)]
    external_references: Vec<Value>,
}

impl TaskRepository {
    pub fn new(store: Store, config: ProjectConfig) -> Result<Self, StoreError> {
        config
            .validate()
            .map_err(|error| StoreError::InvalidConfig(error.to_string()))?;
        Ok(Self { store, config, lock_credential: None })
    }

    pub fn open(
        root: impl AsRef<Path>,
        expected_project_id: Option<&crate::domain::ProjectId>,
        config: ProjectConfig,
    ) -> Result<Self, StoreError> {
        Self::new(Store::open(root, expected_project_id)?, config)
    }

    pub fn config(&self) -> &ProjectConfig {
        &self.config
    }

    pub fn with_lock_grant(&self, grant: &LockGrant) -> Self {
        let mut repository = self.clone();
        repository.lock_credential = Some(LockCredential::from(grant));
        repository
    }

    pub fn lock_service(&self) -> Result<TaskLockService, StoreError> {
        TaskLockService::new(self.store.clone(), self.config.clone())
    }

    fn check_lock(
        &self,
        connection: &Connection,
        task_id: TaskId,
    ) -> Result<Option<i64>, StoreError> {
        check_lock_access(
            connection,
            task_id,
            self.lock_credential.as_ref(),
            SystemClock.now_millis(),
        )
    }

    pub fn create(
        &self,
        mut new_task: NewTask,
        actor: &ActorContext,
    ) -> Result<Task, StoreError> {
        if let Err(error) =
            validate_task_input(&new_task.title, &new_task.labels, &new_task.commit_refs)
        {
            return self.reject_operation(
                None, actor, "task.create", "invalid_request",
                serde_json::to_string(&new_task).ok(), error,
            );
        }

        if !self.config.statuses.iter().any(|status| {
            status.id == new_task.status_id.as_str() && status.enabled
        }) {
            return self.reject_operation(
                None, actor, "task.create", "invalid_status_transition",
                serde_json::to_string(&new_task).ok(),
                StoreError::InvalidStatusTransition(format!(
                    "Status '{}' is missing or disabled",
                    new_task.status_id
                )),
            );
        }

        let mapped_outcome = self.config.outcome_for_status(new_task.status_id.as_str());
        if new_task.outcome.is_some() && new_task.outcome != mapped_outcome {
            return self.reject_operation(
                None, actor, "task.create", "invalid_request",
                serde_json::to_string(&new_task).ok(),
                StoreError::InvalidTask(
                    "Task outcome is determined by its configured status or explicit completion/cancellation operation".into(),
                ),
            );
        }
        new_task.outcome = mapped_outcome;

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

    /// Imports one validated legacy task with its original numeric identity and creation time.
    /// This is only intended for a fresh staging store during explicit migration.
    pub fn import_legacy_task(
        &self,
        raw_id: i64,
        title: String,
        status_id: StatusId,
        outcome: Option<TaskOutcome>,
        body: String,
        created_at: String,
        labels: Vec<String>,
        commit_refs: Vec<String>,
        actor: &ActorContext,
    ) -> Result<Task, StoreError> {
        validate_task_input(&title, &labels, &commit_refs)?;
        let id = TaskId::new(raw_id)
            .map_err(|error| StoreError::InvalidTask(error.to_string()))?;
        if !self.config.statuses.iter().any(|status| status.id == status_id.as_str()) {
            return Err(StoreError::InvalidConfig(format!(
                "Legacy status '{}' is not configured", status_id
            )));
        }
        if self.config.outcome_for_status(status_id.as_str()) != outcome {
            return Err(StoreError::InvalidConfig(format!(
                "Legacy outcome for status '{}' does not match project configuration", status_id
            )));
        }

        let mut connection = self.store.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT INTO tasks(task_id, title, status_id, outcome, body, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
            params![
                id.get(),
                title.trim(),
                status_id.as_str(),
                outcome_as_str(outcome),
                body,
                created_at
            ],
        )?;
        insert_labels(&transaction, id, &labels)?;
        insert_commit_refs(&transaction, id, &commit_refs)?;
        let task = load_task(&transaction, id)?;
        let snapshot = snapshot_json(&transaction, &task)?;
        insert_version(
            &transaction, id, 1, &snapshot, &created_at, actor.actor_id().as_str()
        )?;
        insert_history(
            &transaction,
            Some(id),
            actor.actor_id().as_str(),
            &created_at,
            "migration_import",
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
        self.check_lock(&transaction, id)?;
        let task = load_task(&transaction, id)?;
        transaction.commit()?;
        Ok(task)
    }

    pub fn list(&self) -> Result<Vec<Task>, StoreError> {
        self.list_with_archive_filter(Some(false))
    }

    pub fn list_archived(&self) -> Result<Vec<Task>, StoreError> {
        self.list_with_archive_filter(Some(true))
    }

    pub fn list_all(&self) -> Result<Vec<Task>, StoreError> {
        self.list_with_archive_filter(None)
    }

    fn list_with_archive_filter(&self, archived: Option<bool>) -> Result<Vec<Task>, StoreError> {
        let mut connection = self.store.connection()?;
        let transaction = connection.transaction()?;
        let sql = match archived {
            Some(false) => "SELECT task_id FROM tasks WHERE archived_at IS NULL ORDER BY task_id ASC",
            Some(true) => "SELECT task_id FROM tasks WHERE archived_at IS NOT NULL ORDER BY task_id ASC",
            None => "SELECT task_id FROM tasks ORDER BY task_id ASC",
        };
        let ids = {
            let mut statement = transaction.prepare(sql)?;
            let rows = statement.query_map([], |row| row.get::<_, i64>(0))?;
            rows.collect::<Result<Vec<_>, _>>()?
        };

        let mut tasks = Vec::with_capacity(ids.len());
        for raw_id in ids {
            let id = TaskId::new(raw_id)
                .map_err(|error| StoreError::InvalidTask(error.to_string()))?;
            match self.check_lock(&transaction, id) {
                Ok(_) => tasks.push(load_task(&transaction, id)?),
                Err(StoreError::TaskLocked | StoreError::LockExpired | StoreError::StaleLock) => {}
                Err(error) => return Err(error),
            }
        }
        transaction.commit()?;
        Ok(tasks)
    }

    pub fn archive(&self, task_id: TaskId, actor: &ActorContext) -> Result<TaskArchiveResult, StoreError> {
        self.set_archive_state(task_id, true, actor)
    }

    pub fn restore(&self, task_id: TaskId, actor: &ActorContext) -> Result<TaskArchiveResult, StoreError> {
        self.set_archive_state(task_id, false, actor)
    }

    fn set_archive_state(
        &self,
        task_id: TaskId,
        archived: bool,
        actor: &ActorContext,
    ) -> Result<TaskArchiveResult, StoreError> {
        let operation = if archived { "task.archive" } else { "task.restore" };
        let mut connection = self.store.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        self.check_lock(&transaction, task_id)?;
        let previous = match load_task(&transaction, task_id) {
            Ok(task) => task,
            Err(error @ StoreError::TaskNotFound(_)) => {
                drop(transaction);
                return self.reject_operation(None, actor, operation, "task_not_found", None, error);
            }
            Err(error) => return Err(error),
        };

        let already_in_state = previous.archived_at.is_some() == archived;
        if already_in_state {
            let snapshot = snapshot_json(&transaction, &previous)?;
            let now = now_string();
            insert_history(
                &transaction,
                Some(task_id),
                actor.actor_id().as_str(),
                &now,
                operation,
                Some(&snapshot),
                Some(&snapshot),
                HistoryOutcome::Succeeded,
                None,
                Some(r#"{"no_op":true}"#),
                None,
            )?;
            transaction.commit()?;
            return Ok(TaskArchiveResult { task: previous, no_op: true });
        }

        let mut updated = previous.clone();
        if archived {
            updated.archived_at = Some(now_string());
            updated.archived_by = Some(actor.actor_id().clone());
        } else {
            updated.archived_at = None;
            updated.archived_by = None;
        }
        let task = persist_task_change(
            &transaction,
            &previous,
            updated,
            actor,
            operation,
            None,
            None,
        )?;
        transaction.commit()?;
        Ok(TaskArchiveResult { task, no_op: false })
    }

    pub fn update(
        &self,
        task: &Task,
        actor: &ActorContext,
    ) -> Result<Task, StoreError> {
        if let Err(error) = validate_task_input(&task.title, &task.labels, &task.commit_refs) {
            return self.reject_operation(
                Some(task.id), actor, "task.update", "invalid_request",
                serde_json::to_string(task).ok(), error,
            );
        }

        let mut connection = self.store.connection()?;
        let transaction =
            connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        self.check_lock(&transaction, task.id)?;
        let previous = match load_task(&transaction, task.id) {

            Ok(previous) => previous,
            Err(error @ StoreError::TaskNotFound(_)) => {
                drop(transaction);
                return self.reject_operation(
                    None, actor, "task.update", "task_not_found",
                    serde_json::to_string(task).ok(), error,
                );
            }
            Err(error) => return Err(error),
        };

        if previous.archived_at.is_some() {
            drop(transaction);
            return self.reject_operation(
                Some(task.id), actor, "task.update", "task_archived",
                serde_json::to_string(task).ok(), StoreError::TaskArchived,
            );
        }

        if task.dependencies != previous.dependencies {
            drop(transaction);
            return self.reject_operation(
                Some(task.id), actor, "task.update", "invalid_request",
                serde_json::to_string(task).ok(),
                StoreError::InvalidTask(
                    "Use dependency operations to change task dependencies".into(),
                ),
            );
        }

        let status_changed = previous.status_id != task.status_id;
        if status_changed && previous.outcome.is_some() {
            drop(transaction);
            return self.reject_operation(
                Some(task.id), actor, "task.update", "task_terminal",
                serde_json::to_string(task).ok(), StoreError::TaskTerminal,
            );
        }
        if task.outcome != previous.outcome {
            drop(transaction);
            return self.reject_operation(
                Some(task.id), actor, "task.update", "invalid_request",
                serde_json::to_string(task).ok(),
                StoreError::InvalidTask(
                    "Use complete, cancel, or reopen to change task outcome".into(),
                ),
            );
        }

        let mut updated = task.clone();
        updated.created_at = previous.created_at.clone();
        updated.updated_at = previous.updated_at.clone();
        if status_changed {
            if !self.config.is_transition_allowed(
                previous.status_id.as_str(),
                task.status_id.as_str(),
            ) {
                drop(transaction);
                return self.reject_operation(
                    Some(task.id), actor, "task.update", "invalid_status_transition",
                    serde_json::to_string(task).ok(),
                    StoreError::InvalidStatusTransition(format!(
                        "{} -> {}",
                        previous.status_id, task.status_id
                    )),
                );
            }
            updated.outcome = self.config.outcome_for_status(task.status_id.as_str());
        }

        if same_task_state(&previous, &updated) {
            drop(transaction);
            return Ok(previous);
        }

        let revision = if status_changed {
            Some(self.config_revision()?)
        } else {
            None
        };
        let result = persist_task_change(
            &transaction,
            &previous,
            updated,
            actor,
            "task.update",
            None,
            revision.as_deref(),
        )?;
        transaction.commit()?;
        Ok(result)
    }

    pub fn set_status(
        &self,
        task_id: TaskId,
        target: StatusId,
        actor: &ActorContext,
    ) -> Result<Task, StoreError> {
        let mut connection = self.store.connection()?;
        let transaction =
            connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        self.check_lock(&transaction, task_id)?;
        let previous = match load_task(&transaction, task_id) {
            Ok(task) => task,
            Err(error @ StoreError::TaskNotFound(_)) => {
                drop(transaction);
                return self.reject_operation(
                    None, actor, "task.status.set", "task_not_found",
                    Some(serde_json::json!({"status_id": target}).to_string()), error,
                );
            }
            Err(error) => return Err(error),
        };

        if previous.archived_at.is_some() {
            drop(transaction);
            return self.reject_operation(
                Some(task_id), actor, "task.status.set", "task_archived",
                Some(serde_json::json!({"status_id": target}).to_string()), StoreError::TaskArchived,
            );
        }
        if previous.outcome.is_some() {
            drop(transaction);
            return self.reject_operation(
                Some(task_id), actor, "task.status.set", "task_terminal",
                Some(serde_json::json!({"status_id": target}).to_string()),
                StoreError::TaskTerminal,
            );
        }
        if !self.config.is_transition_allowed(previous.status_id.as_str(), target.as_str()) {
            drop(transaction);
            return self.reject_operation(
                Some(task_id), actor, "task.status.set", "invalid_status_transition",
                Some(serde_json::json!({"status_id": target}).to_string()),
                StoreError::InvalidStatusTransition(format!(
                    "{} -> {}",
                    previous.status_id, target
                )),
            );
        }

        let mut updated = previous.clone();
        updated.status_id = target;
        updated.outcome = self.config.outcome_for_status(updated.status_id.as_str());
        let revision = self.config_revision()?;
        let result = persist_task_change(
            &transaction, &previous, updated, actor, "task.status.set",
            None, Some(&revision),
        )?;
        transaction.commit()?;
        Ok(result)
    }

    pub fn complete(
        &self,
        task_id: TaskId,
        actor: &ActorContext,
    ) -> Result<Task, StoreError> {
        self.set_terminal_outcome(task_id, TaskOutcome::Completed, None, actor)
    }

    pub fn cancel(
        &self,
        task_id: TaskId,
        reason: Option<&str>,
        actor: &ActorContext,
    ) -> Result<Task, StoreError> {
        if reason.is_some_and(|value| value.trim().is_empty()) {
            return self.reject_operation(
                Some(task_id), actor, "task.cancel", "invalid_request",
                None,
                StoreError::InvalidTask("Cancellation reason cannot be empty".into()),
            );
        }
        let details = reason.map(|reason| serde_json::json!({"reason": reason}).to_string());
        self.set_terminal_outcome(task_id, TaskOutcome::Cancelled, details.as_deref(), actor)
    }

    pub fn reopen(
        &self,
        task_id: TaskId,
        target_status: Option<StatusId>,
        actor: &ActorContext,
    ) -> Result<Task, StoreError> {
        let mut connection = self.store.connection()?;
        let transaction =
            connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        self.check_lock(&transaction, task_id)?;
        let previous = match load_task(&transaction, task_id) {
            Ok(task) => task,
            Err(error @ StoreError::TaskNotFound(_)) => {
                drop(transaction);
                return self.reject_operation(
                    None, actor, "task.reopen", "task_not_found",
                    None, error,
                );
            }
            Err(error) => return Err(error),
        };

        if previous.archived_at.is_some() {
            drop(transaction);
            return self.reject_operation(
                Some(task_id), actor, "task.reopen", "task_archived",
                None, StoreError::TaskArchived,
            );
        }
        if previous.outcome.is_none() {
            drop(transaction);
            return self.reject_operation(
                Some(task_id), actor, "task.reopen", "invalid_request",
                None,
                StoreError::InvalidTask("Task is not terminal".into()),
            );
        }

        let current_status_is_terminal =
            self.config.outcome_for_status(previous.status_id.as_str()).is_some();
        let target = match target_status {
            Some(target) => target,
            None if current_status_is_terminal => {
                drop(transaction);
                return self.reject_operation(
                    Some(task_id), actor, "task.reopen", "invalid_status_transition",
                    None,
                    StoreError::InvalidStatusTransition(
                        "Reopening a terminal status requires a nonterminal target status".into(),
                    ),
                );
            }
            None => previous.status_id.clone(),
        };

        if target != previous.status_id {
            if !self.config.is_transition_allowed(previous.status_id.as_str(), target.as_str()) {
                drop(transaction);
                return self.reject_operation(
                    Some(task_id), actor, "task.reopen", "invalid_status_transition",
                    Some(serde_json::json!({"status_id": target}).to_string()),
                    StoreError::InvalidStatusTransition(format!(
                        "{} -> {}",
                        previous.status_id, target
                    )),
                );
            }
        }
        if self.config.outcome_for_status(target.as_str()).is_some() {
            drop(transaction);
            return self.reject_operation(
                Some(task_id), actor, "task.reopen", "invalid_status_transition",
                Some(serde_json::json!({"status_id": target}).to_string()),
                StoreError::InvalidStatusTransition(
                    "Reopen target must not have a terminal outcome".into(),
                ),
            );
        }

        let mut updated = previous.clone();
        updated.status_id = target;
        updated.outcome = None;
        let revision = self.config_revision()?;
        let result = persist_task_change(
            &transaction, &previous, updated, actor, "task.reopen",
            None, Some(&revision),
        )?;
        transaction.commit()?;
        Ok(result)
    }

    fn set_terminal_outcome(
        &self,
        task_id: TaskId,
        outcome: TaskOutcome,
        details_json: Option<&str>,
        actor: &ActorContext,
    ) -> Result<Task, StoreError> {
        let operation = match outcome {
            TaskOutcome::Completed => "task.complete",
            TaskOutcome::Cancelled => "task.cancel",
        };
        let mut connection = self.store.connection()?;
        let transaction =
            connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        self.check_lock(&transaction, task_id)?;
        let previous = match load_task(&transaction, task_id) {
            Ok(task) => task,
            Err(error @ StoreError::TaskNotFound(_)) => {
                drop(transaction);
                return self.reject_operation(
                    None, actor, operation, "task_not_found", details_json.map(str::to_owned), error,
                );
            }
            Err(error) => return Err(error),
        };
        if previous.archived_at.is_some() {
            drop(transaction);
            return self.reject_operation(
                Some(task_id), actor, operation, "task_archived",
                details_json.map(str::to_owned), StoreError::TaskArchived,
            );
        }
        if previous.outcome.is_some() {
            drop(transaction);
            return self.reject_operation(
                Some(task_id), actor, operation, "task_terminal", details_json.map(str::to_owned),
                StoreError::TaskTerminal,
            );
        }

        let mut updated = previous.clone();
        updated.outcome = Some(outcome);
        let result = persist_task_change(
            &transaction, &previous, updated, actor, operation,
            details_json, None,
        )?;
        transaction.commit()?;
        Ok(result)
    }

    fn reject_operation<T>(
        &self,
        task_id: Option<TaskId>,
        actor: &ActorContext,
        operation: &str,
        error_code: &str,
        requested_json: Option<String>,
        error: StoreError,
    ) -> Result<T, StoreError> {
        self.record_rejection(
            task_id, actor, operation, error_code, requested_json.as_deref(),
        )?;
        Err(error)
    }

    fn config_revision(&self) -> Result<String, StoreError> {
        let serialized = self.config.to_toml()
            .map_err(|error| StoreError::InvalidConfig(error.to_string()))?;
        Ok(hex::encode(Sha256::digest(serialized.as_bytes())))
    }

    pub fn add_dependency(
        &self,
        task_id: TaskId,
        depends_on: TaskId,
        actor: &ActorContext,
    ) -> Result<(), StoreError> {
        let mut connection = self.store.connection()?;
        let transaction =
            connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        self.check_lock(&transaction, task_id)?;
        let previous = match load_task(&transaction, task_id) {
            Ok(task) => task,
            Err(error @ StoreError::TaskNotFound(_)) => {
                drop(transaction);
                return self.reject_operation(
                    None, actor, "task.dependency.add", "task_not_found",
                    Some(serde_json::json!({"depends_on": depends_on}).to_string()), error,
                );
            }
            Err(error) => return Err(error),
        };
        if previous.archived_at.is_some() {
            drop(transaction);
            return self.reject_operation(
                Some(task_id), actor, "task.dependency.add", "task_archived",
                Some(serde_json::json!({"depends_on": depends_on}).to_string()), StoreError::TaskArchived,
            );
        }
        match load_task(&transaction, depends_on) {
            Ok(_) => {}
            Err(StoreError::TaskNotFound(_)) => {
                drop(transaction);
                return self.reject_operation(
                    Some(task_id), actor, "task.dependency.add", "dependency_not_found",
                    Some(serde_json::json!({"depends_on": depends_on}).to_string()),
                    StoreError::DependencyNotFound(depends_on.get()),
                );
            }
            Err(error) => return Err(error),
        }

        if task_id == depends_on {
            drop(transaction);
            return self.reject_operation(
                Some(task_id), actor, "task.dependency.add", "dependency_cycle",
                Some(serde_json::json!({"depends_on": depends_on}).to_string()),
                StoreError::DependencyCycle,
            );
        }

        let exists: bool = transaction.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM task_dependencies
                WHERE task_id = ?1 AND depends_on_task_id = ?2
             )",
            params![task_id.get(), depends_on.get()],
            |row| row.get(0),
        )?;
        if exists {
            drop(transaction);
            return Ok(());
        }

        let creates_cycle: bool = transaction.query_row(
            "WITH RECURSIVE reachable(task_id) AS (
                SELECT depends_on_task_id
                FROM task_dependencies
                WHERE task_id = ?1
                UNION
                SELECT dependency.depends_on_task_id
                FROM task_dependencies dependency
                JOIN reachable current ON dependency.task_id = current.task_id
             )
             SELECT EXISTS(SELECT 1 FROM reachable WHERE task_id = ?2)",
            params![depends_on.get(), task_id.get()],
            |row| row.get(0),
        )?;
        if creates_cycle {
            drop(transaction);
            return self.reject_operation(
                Some(task_id), actor, "task.dependency.add", "dependency_cycle",
                Some(serde_json::json!({"depends_on": depends_on}).to_string()),
                StoreError::DependencyCycle,
            );
        }

        let previous_snapshot = snapshot_json(&transaction, &previous)?;
        let now = now_string();
        transaction.execute(
            "INSERT INTO task_dependencies(task_id, depends_on_task_id, created_at)
             VALUES (?1, ?2, ?3)",
            params![task_id.get(), depends_on.get(), now],
        )?;
        let updated = load_task(&transaction, task_id)?;
        let updated_snapshot = snapshot_json(&transaction, &updated)?;
        let version_no = next_version_no(&transaction, task_id)?;
        insert_version(
            &transaction, task_id, version_no, &updated_snapshot, &now,
            actor.actor_id().as_str(),
        )?;
        insert_history(
            &transaction, Some(task_id), actor.actor_id().as_str(), &now,
            "task.dependency.add", Some(&previous_snapshot), Some(&updated_snapshot),
            HistoryOutcome::Succeeded, None, None, None,
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn remove_dependency(
        &self,
        task_id: TaskId,
        depends_on: TaskId,
        actor: &ActorContext,
    ) -> Result<(), StoreError> {
        let mut connection = self.store.connection()?;
        let transaction =
            connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        self.check_lock(&transaction, task_id)?;
        let previous = match load_task(&transaction, task_id) {
            Ok(task) => task,
            Err(error @ StoreError::TaskNotFound(_)) => {
                drop(transaction);
                return self.reject_operation(
                    None, actor, "task.dependency.remove", "task_not_found",
                    Some(serde_json::json!({"depends_on": depends_on}).to_string()), error,
                );
            }
            Err(error) => return Err(error),
        };

        if previous.archived_at.is_some() {
            drop(transaction);
            return self.reject_operation(
                Some(task_id), actor, "task.dependency.remove", "task_archived",
                Some(serde_json::json!({"depends_on": depends_on}).to_string()), StoreError::TaskArchived,
            );
        }
        let previous_snapshot = snapshot_json(&transaction, &previous)?;
        let changed = transaction.execute(
            "DELETE FROM task_dependencies
             WHERE task_id = ?1 AND depends_on_task_id = ?2",
            params![task_id.get(), depends_on.get()],
        )?;
        if changed == 0 {
            drop(transaction);
            return self.reject_operation(
                Some(task_id), actor, "task.dependency.remove", "dependency_not_found",
                Some(serde_json::json!({"depends_on": depends_on}).to_string()),
                StoreError::DependencyNotFound(depends_on.get()),
            );
        }

        let updated = load_task(&transaction, task_id)?;
        let updated_snapshot = snapshot_json(&transaction, &updated)?;
        let now = now_string();
        let version_no = next_version_no(&transaction, task_id)?;
        insert_version(
            &transaction, task_id, version_no, &updated_snapshot, &now,
            actor.actor_id().as_str(),
        )?;
        insert_history(
            &transaction, Some(task_id), actor.actor_id().as_str(), &now,
            "task.dependency.remove", Some(&previous_snapshot), Some(&updated_snapshot),
            HistoryOutcome::Succeeded, None, None, None,
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn list_dependencies(&self, task_id: TaskId) -> Result<Vec<TaskId>, StoreError> {
        Ok(self.get(task_id)?.dependencies)
    }

    pub fn list_dependency_states(&self, task_id: TaskId) -> Result<Vec<DependencyState>, StoreError> {
        let mut connection = self.store.connection()?;
        let transaction = connection.transaction()?;
        self.check_lock(&transaction, task_id)?;
        let ids = {
            let mut statement = transaction.prepare(
                "SELECT depends_on_task_id FROM task_dependencies
                 WHERE task_id = ?1 ORDER BY depends_on_task_id ASC",
            )?;
            let rows = statement.query_map(params![task_id.get()], |row| row.get::<_, i64>(0))?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let mut states = Vec::with_capacity(ids.len());
        for raw_id in ids {
            let dependency_id = TaskId::new(raw_id)
                .map_err(|error| StoreError::InvalidTask(error.to_string()))?;
            match self.check_lock(&transaction, dependency_id) {
                Ok(_) => {
                    let task = load_task(&transaction, dependency_id)?;
                    states.push(DependencyState {
                        task_id: dependency_id,
                        status_id: Some(task.status_id),
                        outcome: task.outcome,
                        archived: Some(task.archived_at.is_some()),
                        locked: false,
                    });
                }
                Err(StoreError::TaskLocked | StoreError::LockExpired | StoreError::StaleLock) => {
                    states.push(DependencyState {
                        task_id: dependency_id,
                        status_id: None,
                        outcome: None,
                        archived: None,
                        locked: true,
                    });
                }
                Err(error) => return Err(error),
            }
        }
        transaction.commit()?;
        Ok(states)
    }

    pub fn list_versions(&self, task_id: TaskId) -> Result<Vec<TaskVersion>, StoreError> {
        let mut connection = self.store.connection()?;
        let transaction = connection.transaction()?;
        self.check_lock(&transaction, task_id)?;
        let versions = {
            let mut statement = transaction.prepare(
                "SELECT version_id, task_id, version_no, snapshot_json, created_at, actor_id
                 FROM task_versions WHERE task_id = ?1 ORDER BY version_no ASC",
            )?;
            let rows = statement.query_map(params![task_id.get()], map_version)?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        transaction.commit()?;
        Ok(versions)
    }

    pub fn get_version(
        &self,
        task_id: TaskId,
        version_no: i64,
    ) -> Result<TaskVersion, StoreError> {
        let mut connection = self.store.connection()?;
        let transaction = connection.transaction()?;
        self.check_lock(&transaction, task_id)?;
        let version = transaction
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
            })?;
        transaction.commit()?;
        Ok(version)
    }

    pub fn get_history(&self, task_id: TaskId) -> Result<Vec<HistoryEntry>, StoreError> {
        let mut connection = self.store.connection()?;
        let transaction = connection.transaction()?;
        self.check_lock(&transaction, task_id)?;
        let history = {
            let mut statement = transaction.prepare(
                "SELECT event_id, task_id, actor_id, occurred_at, operation,
                        previous_json, requested_json, outcome, error_code,
                        details_json, config_revision, fencing_generation
                 FROM operation_history WHERE task_id = ?1 ORDER BY event_id ASC",
            )?;
            let rows = statement.query_map(params![task_id.get()], map_history)?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        transaction.commit()?;
        Ok(history)
    }

    pub fn get_store_history(&self) -> Result<Vec<HistoryEntry>, StoreError> {
        let mut connection = self.store.connection()?;
        let transaction = connection.transaction()?;
        let entries = {
            let mut statement = transaction.prepare(
                "SELECT event_id, task_id, actor_id, occurred_at, operation,
                        previous_json, requested_json, outcome, error_code,
                        details_json, config_revision, fencing_generation
                 FROM operation_history ORDER BY event_id ASC",
            )?;
            let rows = statement.query_map([], map_history)?;
            rows.collect::<Result<Vec<_>, _>>()?
        };

        let mut visible = Vec::with_capacity(entries.len());
        for entry in entries {
            match entry.task_id {
                None => visible.push(entry),
                Some(task_id) => match self.check_lock(&transaction, task_id) {
                    Ok(_) => visible.push(entry),
                    Err(StoreError::TaskLocked | StoreError::LockExpired | StoreError::StaleLock) => {}
                    Err(error) => return Err(error),
                },
            }
        }
        transaction.commit()?;
        Ok(visible)
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
        self.check_lock(&transaction, task_id)?;
        let task = match load_task(&transaction, task_id) {
            Ok(task) => task,
            Err(error @ StoreError::TaskNotFound(_)) => {
                drop(transaction);
                return self.reject_operation(None, actor, "task.version.approve", "task_not_found", None, error);
            }
            Err(error) => return Err(error),
        };
        if task.archived_at.is_some() {
            drop(transaction);
            return self.reject_operation(
                Some(task_id), actor, "task.version.approve", "task_archived",
                Some(serde_json::json!({"version_no": version_no}).to_string()), StoreError::TaskArchived,
            );
        }
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
                None,
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
        let mut connection = self.store.connection()?;
        let transaction = connection.transaction()?;
        self.check_lock(&transaction, task_id)?;
        let baseline = transaction
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
                Ok::<TaskBaseline, StoreError>(TaskBaseline {
                    task_id: TaskId::new(raw_task_id)
                        .map_err(|error| StoreError::InvalidTask(error.to_string()))?,
                    version_id,
                    version_no,
                    approved_at,
                    approved_by: ActorId::new(approved_by)
                        .map_err(|error| StoreError::InvalidTask(error.to_string()))?,
                })
            })
            .transpose()?;
        transaction.commit()?;
        Ok(baseline)
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
        push_change(&mut changes, "archived_at", &before.archived_at, &after.archived_at)?;
        push_change(&mut changes, "archived_by", &before.archived_by, &after.archived_by)?;

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

    pub fn external_reference_add(
        &self, task_id: TaskId, namespace: &str, value: &str, relation: &str,
        scope: &str, actor: &ActorContext,
    ) -> Result<Value, StoreError> {
        if namespace.trim().is_empty() || namespace.len() > 128 ||
           value.trim().is_empty() || value.len() > 512 ||
           relation.trim().is_empty() || relation.len() > 128 ||
           !matches!(scope, "task" | "project") {
            return Err(StoreError::InvalidTask("Invalid external reference fields".into()));
        }
        let mut connection = self.store.connection()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        self.check_lock(&tx, task_id)?;
        let task = load_task(&tx, task_id)?;
        if task.archived_at.is_some() { return Err(StoreError::TaskArchived); }
        let mut statement = tx.prepare(
            "SELECT reference_id, task_id, relation, scope FROM task_external_references
             WHERE namespace = ?1 AND value = ?2 AND removed_at IS NULL"
        )?;
        let existing = statement.query_map(params![namespace.trim(), value.trim()], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?, row.get::<_, String>(3)?))
        })?.collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        for (reference_id, existing_task, existing_relation, existing_scope) in existing {
            if existing_task == task_id.get() {
                if existing_relation == relation.trim() && existing_scope == scope {
                    tx.commit()?;
                    return Ok(json!({"reference_id": reference_id, "task_id": task_id,
                        "namespace": namespace.trim(), "value": value.trim(),
                        "relation": relation.trim(), "scope": scope, "no_op": true}));
                }
                return Err(StoreError::InvalidTask("external_ref_conflict".into()));
            }
            if scope == "project" || existing_scope == "project" {
                return Err(StoreError::InvalidTask("external_ref_conflict".into()));
            }
        }
        let mut bytes = [0u8; 16];
        getrandom::fill(&mut bytes).map_err(|e| StoreError::InvalidTask(format!("Cannot generate reference ID: {e}")))?;
        let reference_id = hex::encode(bytes);
        let now = now_string();
        let before = snapshot_json(&tx, &task)?;
        tx.execute(
            "INSERT INTO task_external_references(reference_id, task_id, namespace, value, relation, scope, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![reference_id, task_id.get(), namespace.trim(), value.trim(), relation.trim(), scope, now],
        )?;
        let after = snapshot_json(&tx, &task)?;
        record_extension_change(&tx, task_id, actor, "task.external-ref.add", &before, &after, &now)?;
        tx.commit()?;
        Ok(json!({"reference_id": reference_id, "task_id": task_id,
            "namespace": namespace.trim(), "value": value.trim(),
            "relation": relation.trim(), "scope": scope, "no_op": false}))
    }

    pub fn external_reference_list(&self, task_id: TaskId, include_removed: bool) -> Result<Value, StoreError> {
        let mut connection = self.store.connection()?;
        let tx = connection.transaction()?;
        self.check_lock(&tx, task_id)?;
        let sql = if include_removed {
            "SELECT reference_id, namespace, value, relation, scope, created_at, removed_at, removed_by
             FROM task_external_references WHERE task_id = ?1 ORDER BY created_at, reference_id"
        } else {
            "SELECT reference_id, namespace, value, relation, scope, created_at, removed_at, removed_by
             FROM task_external_references WHERE task_id = ?1 AND removed_at IS NULL ORDER BY created_at, reference_id"
        };
        let _task = load_task(&tx, task_id)?;
        let mut statement = tx.prepare(sql)?;
        let rows = statement.query_map(params![task_id.get()], |row| Ok(json!({
            "reference_id": row.get::<_, String>(0)?, "namespace": row.get::<_, String>(1)?,
            "value": row.get::<_, String>(2)?, "relation": row.get::<_, String>(3)?,
            "scope": row.get::<_, String>(4)?, "created_at": row.get::<_, String>(5)?,
            "removed_at": row.get::<_, Option<String>>(6)?, "removed_by": row.get::<_, Option<String>>(7)?
        })))?.collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        tx.commit()?;
        Ok(json!({"references": rows}))
    }

    pub fn external_reference_remove(&self, task_id: TaskId, reference_id: &str, actor: &ActorContext) -> Result<Value, StoreError> {
        let mut connection = self.store.connection()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        self.check_lock(&tx, task_id)?;
        let task = load_task(&tx, task_id)?;
        let state: Option<Option<String>> = tx.query_row(
            "SELECT removed_at FROM task_external_references WHERE reference_id = ?1 AND task_id = ?2",
            params![reference_id, task_id.get()], |row| row.get(0)
        ).optional()?;
        let removed_at = state.ok_or_else(|| StoreError::InvalidTask("external_ref_not_found".into()))?;
        if removed_at.is_some() {
            tx.commit()?;
            return Ok(json!({"reference_id": reference_id, "no_op": true}));
        }
        let now = now_string();
        let before = snapshot_json(&tx, &task)?;
        tx.execute(
            "UPDATE task_external_references SET removed_at = ?1, removed_by = ?2
             WHERE reference_id = ?3 AND task_id = ?4",
            params![now, actor.actor_id().as_str(), reference_id, task_id.get()],
        )?;
        let after = snapshot_json(&tx, &task)?;
        record_extension_change(&tx, task_id, actor, "task.external-ref.remove", &before, &after, &now)?;
        tx.commit()?;
        Ok(json!({"reference_id": reference_id, "no_op": false}))
    }

    pub fn review_add(
        &self, task_id: TaskId, review_id: &str, version_no: i64, reviewer: &str,
        outcome: &str, comments: &[Value], actor: &ActorContext,
    ) -> Result<Value, StoreError> {
        if review_id.trim().is_empty() || review_id.len() > 128 ||
           reviewer.trim().is_empty() || reviewer.len() > 256 ||
           version_no <= 0 || !matches!(outcome, "approved" | "changes_requested") {
            return Err(StoreError::InvalidTask("Invalid review fields".into()));
        }
        let comments_json = serde_json::to_string(comments)
            .map_err(|e| StoreError::InvalidTask(format!("Invalid review comments: {e}")))?;
        let mut connection = self.store.connection()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        self.check_lock(&tx, task_id)?;
        let task = load_task(&tx, task_id)?;
        if task.archived_at.is_some() { return Err(StoreError::TaskArchived); }
        let existing: Option<(i64, String, String, String, String)> = tx.query_row(
            "SELECT version_no, reviewer, outcome, comments_json, actor_id FROM task_reviews
             WHERE task_id = ?1 AND review_id = ?2",
            params![task_id.get(), review_id.trim()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?))
        ).optional()?;
        if let Some((v, r, o, c, a)) = existing {
            if v == version_no && r == reviewer.trim() && o == outcome && c == comments_json && a == actor.actor_id().as_str() {
                tx.commit()?;
                let revision_count: i64 = tx.query_row(
                    "SELECT revision_count FROM tasks WHERE task_id = ?1", params![task_id.get()], |row| row.get(0)
                )?;
                let task_version_no: i64 = tx.query_row(
                    "SELECT COALESCE(MAX(version_no), 0) FROM task_versions WHERE task_id = ?1",
                    params![task_id.get()], |row| row.get(0)
                )?;
                tx.commit()?;
                return Ok(json!({"review_id": review_id, "task_id": task_id, "reviewed_version_no": version_no,
                    "task_version_no": task_version_no, "revision_count": revision_count,
                    "reviewer": reviewer.trim(), "outcome": outcome, "comments": comments, "no_op": true}));
            }
            return Err(StoreError::InvalidTask("review_id_conflict".into()));
        }
        let version_exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM task_versions WHERE task_id = ?1 AND version_no = ?2)",
            params![task_id.get(), version_no], |row| row.get(0)
        )?;
        if !version_exists { return Err(StoreError::InvalidTask("review_version_not_found".into())); }
        let now = now_string();
        let before = snapshot_json(&tx, &task)?;
        tx.execute(
            "INSERT INTO task_reviews(task_id, review_id, version_no, reviewer, outcome, comments_json, actor_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![task_id.get(), review_id.trim(), version_no, reviewer.trim(), outcome,
                comments_json, actor.actor_id().as_str(), now],
        )?;
        if outcome == "changes_requested" {
            tx.execute("UPDATE tasks SET revision_count = revision_count + 1 WHERE task_id = ?1", params![task_id.get()])?;
        }
        let after = snapshot_json(&tx, &task)?;
        record_extension_change(&tx, task_id, actor, "task.review.add", &before, &after, &now)?;
        let task_version_no = next_version_no(&tx, task_id)? - 1;
        let revision_count: i64 = tx.query_row(
            "SELECT revision_count FROM tasks WHERE task_id = ?1", params![task_id.get()], |row| row.get(0)
        )?;
        tx.commit()?;
        Ok(json!({"review_id": review_id, "task_id": task_id, "reviewed_version_no": version_no,
            "task_version_no": task_version_no, "revision_count": revision_count,
            "reviewer": reviewer.trim(), "outcome": outcome, "comments": comments, "no_op": false}))
    }

    pub fn review_list(&self, task_id: TaskId, before_record_id: Option<i64>, limit: Option<usize>) -> Result<Value, StoreError> {
        let limit = limit.unwrap_or(100).min(i64::MAX as usize);
        if limit == 0 { return Err(StoreError::InvalidTask("limit must be a positive integer".into())); }
        let mut connection = self.store.connection()?;
        let tx = connection.transaction()?;
        self.check_lock(&tx, task_id)?;
        let _task = load_task(&tx, task_id)?;
        let mut statement = tx.prepare(
            "SELECT review_record_id, review_id, version_no, reviewer, outcome, comments_json, actor_id, created_at
             FROM task_reviews WHERE task_id = ?1 AND (?2 IS NULL OR review_record_id < ?2)
             ORDER BY review_record_id DESC LIMIT ?3"
        )?;
        let rows = statement.query_map(params![task_id.get(), before_record_id, limit as i64], |row| {
            let comments: String = row.get(5)?;
            let comments: Value = serde_json::from_str(&comments).map_err(|e|
                rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(e)))?;
            Ok(json!({"record_id": row.get::<_, i64>(0)?, "review_id": row.get::<_, String>(1)?,
                "version_no": row.get::<_, i64>(2)?, "reviewer": row.get::<_, String>(3)?,
                "outcome": row.get::<_, String>(4)?, "comments": comments,
                "actor_id": row.get::<_, String>(6)?, "created_at": row.get::<_, String>(7)?}))
        })?.collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        tx.commit()?;
        Ok(json!({"reviews": rows}))
    }

    pub fn set_final_commit(&self, task_id: TaskId, sha: &str, actor: &ActorContext) -> Result<Value, StoreError> {
        if !matches!(sha.len(), 40 | 64) || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(StoreError::InvalidTask("Invalid full Git commit SHA".into()));
        }
        let sha = sha.to_ascii_lowercase();
        let mut connection = self.store.connection()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        self.check_lock(&tx, task_id)?;
        let task = load_task(&tx, task_id)?;
        if task.archived_at.is_some() { return Err(StoreError::TaskArchived); }
        let current: Option<String> = tx.query_row(
            "SELECT final_commit_sha FROM tasks WHERE task_id = ?1", params![task_id.get()], |row| row.get(0)
        )?;
        if current.as_deref() == Some(sha.as_str()) {
            tx.commit()?;
            return Ok(json!({"task_id": task_id, "final_commit_sha": sha, "no_op": true}));
        }
        if task.outcome.is_some() { return Err(StoreError::TaskTerminal); }
        if current.is_some() {
            let last_set: Option<i64> = tx.query_row(
                "SELECT MAX(event_id) FROM operation_history WHERE task_id = ?1 AND operation = 'task.commit.finalize'",
                params![task_id.get()], |row| row.get(0)
            )?;
            let reopened_after_set: bool = match last_set {
                Some(event_id) => tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM operation_history WHERE task_id = ?1
                     AND operation = 'task.reopen' AND event_id > ?2)",
                    params![task_id.get(), event_id], |row| row.get(0)
                )?,
                None => false,
            };
            if !reopened_after_set {
                return Err(StoreError::TaskTerminal);
            }
        }
        let now = now_string();
        let before = snapshot_json(&tx, &task)?;
        tx.execute("UPDATE tasks SET final_commit_sha = ?1 WHERE task_id = ?2", params![sha, task_id.get()])?;
        let after = snapshot_json(&tx, &task)?;
        record_extension_change(&tx, task_id, actor, "task.commit.finalize", &before, &after, &now)?;
        tx.commit()?;
        Ok(json!({"task_id": task_id, "final_commit_sha": sha, "no_op": false}))
    }

    pub fn extension_metadata(&self, task_id: TaskId) -> Result<Value, StoreError> {
        let mut connection = self.store.connection()?;
        let tx = connection.transaction()?;
        self.check_lock(&tx, task_id)?;
        let (revision_count, final_commit_sha): (i64, Option<String>) = tx.query_row(
            "SELECT revision_count, final_commit_sha FROM tasks WHERE task_id = ?1",
            params![task_id.get()], |row| Ok((row.get(0)?, row.get(1)?))
        )?;
        let mut statement = tx.prepare(
            "SELECT reference_id, namespace, value, relation, scope, created_at
             FROM task_external_references WHERE task_id = ?1 AND removed_at IS NULL
             ORDER BY created_at, reference_id"
        )?;
        let external_references = statement.query_map(params![task_id.get()], |row| Ok(json!({
            "reference_id": row.get::<_, String>(0)?, "namespace": row.get::<_, String>(1)?,
            "value": row.get::<_, String>(2)?, "relation": row.get::<_, String>(3)?,
            "scope": row.get::<_, String>(4)?, "created_at": row.get::<_, String>(5)?
        })))?.collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        tx.commit()?;
        Ok(json!({"revision_count": revision_count, "final_commit_sha": final_commit_sha,
            "external_references": external_references}))
    }

}


fn record_extension_change(
    tx: &Transaction<'_>, task_id: TaskId, actor: &ActorContext,
    operation: &str, previous: &str, requested: &str, now: &str,
) -> Result<(), StoreError> {
    let version_no = next_version_no(tx, task_id)?;
    insert_version(tx, task_id, version_no, requested, now, actor.actor_id().as_str())?;
    insert_history(tx, Some(task_id), actor.actor_id().as_str(), now, operation,
        Some(previous), Some(requested), HistoryOutcome::Succeeded, None, None, None)?;
    Ok(())
}

fn persist_task_change(
    transaction: &Transaction<'_>,
    previous: &Task,
    mut updated: Task,
    actor: &ActorContext,
    operation: &str,
    details_json: Option<&str>,
    config_revision: Option<&str>,
) -> Result<Task, StoreError> {
    let now = now_string();
    updated.updated_at = now.clone();
    transaction.execute(
        "UPDATE tasks
         SET title = ?1, status_id = ?2, outcome = ?3, body = ?4, updated_at = ?5,
             archived_at = ?6, archived_by = ?7
         WHERE task_id = ?8",
        params![
            updated.title.trim(),
            updated.status_id.as_str(),
            outcome_as_str(updated.outcome),
            updated.body,
            now,
            updated.archived_at,
            updated.archived_by.as_ref().map(ActorId::as_str),
            updated.id.get()
        ],
    )?;
    transaction.execute(
        "DELETE FROM task_labels WHERE task_id = ?1",
        params![updated.id.get()],
    )?;
    transaction.execute(
        "DELETE FROM task_commit_refs WHERE task_id = ?1",
        params![updated.id.get()],
    )?;
    insert_labels(transaction, updated.id, &updated.labels)?;
    insert_commit_refs(transaction, updated.id, &updated.commit_refs)?;

    let persisted = load_task(transaction, updated.id)?;
    let previous_snapshot = snapshot_json(transaction, previous)?;
    let requested_snapshot = snapshot_json(transaction, &persisted)?;
    let version_no = next_version_no(transaction, persisted.id)?;
    insert_version(
        transaction,
        persisted.id,
        version_no,
        &requested_snapshot,
        &now,
        actor.actor_id().as_str(),
    )?;
    insert_history(
        transaction,
        Some(persisted.id),
        actor.actor_id().as_str(),
        &now,
        operation,
        Some(&previous_snapshot),
        Some(&requested_snapshot),
        HistoryOutcome::Succeeded,
        None,
        details_json,
        config_revision,
    )?;
    Ok(persisted)
}

fn same_task_state(current: &Task, requested: &Task) -> bool {
    current.title == requested.title.trim()
        && current.status_id == requested.status_id
        && current.outcome == requested.outcome
        && current.body == requested.body
        && current.labels == requested.labels.iter().map(|label| label.trim().to_owned()).collect::<Vec<_>>()
        && current.commit_refs == requested.commit_refs
        && current.dependencies == requested.dependencies
        && current.archived_at == requested.archived_at
        && current.archived_by == requested.archived_by
}

fn load_task(connection: &Connection, id: TaskId) -> Result<Task, StoreError> {
    let row = connection
        .query_row(
            "SELECT title, status_id, outcome, body, created_at, updated_at,
                     archived_at, archived_by
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
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
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

    let dependencies = {
        let mut statement = connection.prepare(
            "SELECT depends_on_task_id FROM task_dependencies
             WHERE task_id = ?1 ORDER BY depends_on_task_id ASC",
        )?;
        let rows = statement.query_map(params![id.get()], |row| row.get::<_, i64>(0))?;
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
        archived_at: row.6,
        archived_by: row.7.map(ActorId::new).transpose()
            .map_err(|error| StoreError::InvalidTask(error.to_string()))?,
        labels,
        commit_refs,
        dependencies,
    })
}

fn snapshot_json(connection: &Connection, task: &Task) -> Result<String, StoreError> {
    let revision_count: i64 = connection.query_row(
        "SELECT revision_count FROM tasks WHERE task_id = ?1", params![task.id.get()], |row| row.get(0)
    )?;
    let final_commit_sha: Option<String> = connection.query_row(
        "SELECT final_commit_sha FROM tasks WHERE task_id = ?1", params![task.id.get()], |row| row.get(0)
    )?;
    let external_references = {
        let mut statement = connection.prepare(
            "SELECT reference_id, namespace, value, relation, scope, created_at, removed_at, removed_by
             FROM task_external_references WHERE task_id = ?1 ORDER BY created_at, reference_id"
        )?;
        let rows = statement.query_map(params![task.id.get()], |row| Ok(json!({
            "reference_id": row.get::<_, String>(0)?, "namespace": row.get::<_, String>(1)?,
            "value": row.get::<_, String>(2)?, "relation": row.get::<_, String>(3)?,
            "scope": row.get::<_, String>(4)?, "created_at": row.get::<_, String>(5)?,
            "removed_at": row.get::<_, Option<String>>(6)?, "removed_by": row.get::<_, Option<String>>(7)?
        })))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    let snapshot = TaskSnapshot {
        task_id: task.id,
        title: task.title.clone(),
        status_id: task.status_id.clone(),
        outcome: task.outcome,
        body: task.body.clone(),
        created_at: task.created_at.clone(),
        updated_at: task.updated_at.clone(),
        archived_at: task.archived_at.clone(),
        archived_by: task.archived_by.clone(),
        labels: task.labels.clone(),
        commit_refs: task.commit_refs.clone(),
        dependencies: task.dependencies.clone(),
        revision_count,
        final_commit_sha,
        external_references,
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
    let version_id = row.get(0)?;
    let raw_task_id: i64 = row.get(1)?;
    let version_no = row.get(2)?;
    let snapshot_json = row.get(3)?;
    let created_at = row.get(4)?;
    let actor_raw: String = row.get(5)?;
    let task_id = TaskId::new(raw_task_id).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            1,
            rusqlite::types::Type::Integer,
            Box::new(error),
        )
    })?;
    let actor_id = ActorId::new(actor_raw).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            5,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })?;

    Ok(TaskVersion {
        version_id,
        task_id,
        version_no,
        snapshot_json,
        created_at,
        actor_id,
    })
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
