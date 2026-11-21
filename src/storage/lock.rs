use super::Store;
use super::StoreError;
use crate::{
    config::ProjectConfig,
    domain::{ActorContext, TaskId},
};
use chrono::{SecondsFormat, Utc};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use sha2::{Digest, Sha256};
use std::{
    fmt,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

pub trait Clock: Send + Sync {
    fn now_millis(&self) -> i64;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_millis(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .min(i64::MAX as u128) as i64
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct LockGrant {
    pub task_id: TaskId,
    pub owner_id: String,
    pub token: String,
    pub fencing_generation: i64,
    pub acquired_at: i64,
    pub heartbeat_at: i64,
    pub expires_at: i64,
}

impl fmt::Debug for LockGrant {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LockGrant")
            .field("task_id", &self.task_id)
            .field("owner_id", &self.owner_id)
            .field("token", &"[REDACTED]")
            .field("fencing_generation", &self.fencing_generation)
            .field("acquired_at", &self.acquired_at)
            .field("heartbeat_at", &self.heartbeat_at)
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

#[derive(Clone)]
pub(crate) struct LockCredential {
    pub(crate) owner_id: String,
    pub(crate) token: String,
    pub(crate) fencing_generation: i64,
}

impl From<&LockGrant> for LockCredential {
    fn from(grant: &LockGrant) -> Self {
        Self {
            owner_id: grant.owner_id.clone(),
            token: grant.token.clone(),
            fencing_generation: grant.fencing_generation,
        }
    }
}

impl fmt::Debug for LockCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LockCredential")
            .field("owner_id", &self.owner_id)
            .field("token", &"[REDACTED]")
            .field("fencing_generation", &self.fencing_generation)
            .finish()
    }
}

#[derive(Clone, Debug)]
pub struct TaskLockService {
    store: Store,
    config: ProjectConfig,
    clock: Arc<dyn Clock>,
}

#[derive(Clone, Debug)]
struct LockRecord {
    state: String,
    owner_id: Option<String>,
    token_hash: Option<Vec<u8>>,
    expires_at: Option<i64>,
    fencing_generation: i64,
}

impl TaskLockService {
    pub fn new(store: Store, config: ProjectConfig) -> Result<Self, StoreError> {
        Self::with_clock(store, config, Arc::new(SystemClock))
    }

    pub fn with_clock(
        store: Store,
        config: ProjectConfig,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, StoreError> {
        config
            .validate()
            .map_err(|error| StoreError::InvalidConfig(error.to_string()))?;
        Ok(Self {
            store,
            config,
            clock,
        })
    }

    pub fn acquire(
        &self,
        task_id: TaskId,
        owner_id: &str,
        actor: &ActorContext,
    ) -> Result<LockGrant, StoreError> {
        if let Err(error) = validate_owner_id(owner_id) {
            return self.reject(
                None,
                actor,
                "task.lock.acquire",
                "invalid_request",
                Some(&serde_json::json!({"task_id": task_id}).to_string()),
                error,
            );
        }

        let mut connection = self.store.connection()?;
        let transaction =
            connection.transaction_with_behavior(TransactionBehavior::Exclusive)?;
        let task_exists: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM tasks WHERE task_id = ?1)",
            params![task_id.get()],
            |row| row.get(0),
        )?;
        if !task_exists {
            drop(transaction);
            return self.reject(
                None,
                actor,
                "task.lock.acquire",
                "task_not_found",
                Some(&serde_json::json!({"task_id": task_id}).to_string()),
                StoreError::TaskNotFound(task_id.get()),
            );
        }

        let existing = load_lock(&transaction, task_id)?;
        let generation = match existing {
            None => 1,
            Some(record) if record.state == "released" => {
                record.fencing_generation.checked_add(1).ok_or_else(|| {
                    StoreError::InvalidMetadata("fencing_generation overflow".into())
                })?
            }
            Some(_) => {
                drop(transaction);
                return self.reject(
                    Some(task_id),
                    actor,
                    "task.lock.acquire",
                    "task_locked",
                    Some(&serde_json::json!({"owner_id": owner_id}).to_string()),
                    StoreError::TaskLocked,
                );
            }
        };

        let now = self.clock.now_millis();
        let expires_at = self.expiry_from(now)?;
        let token = generate_token()?;
        let token_hash = token_hash(&token);
        transaction.execute(
            "INSERT INTO task_locks(
                task_id, state, owner_id, token_hash, acquired_at,
                heartbeat_at, expires_at, released_at, fencing_generation
             ) VALUES (?1, 'active', ?2, ?3, ?4, ?4, ?5, NULL, ?6)
             ON CONFLICT(task_id) DO UPDATE SET
                state = 'active',
                owner_id = excluded.owner_id,
                token_hash = excluded.token_hash,
                acquired_at = excluded.acquired_at,
                heartbeat_at = excluded.heartbeat_at,
                expires_at = excluded.expires_at,
                released_at = NULL,
                fencing_generation = excluded.fencing_generation",
            params![
                task_id.get(),
                owner_id,
                token_hash,
                now,
                expires_at,
                generation
            ],
        )?;
        insert_lock_history(
            &transaction,
            task_id,
            actor,
            "task.lock.acquire",
            now,
            generation,
            None,
        )?;
        transaction.commit()?;

        Ok(LockGrant {
            task_id,
            owner_id: owner_id.to_owned(),
            token,
            fencing_generation: generation,
            acquired_at: now,
            heartbeat_at: now,
            expires_at,
        })
    }

    pub fn heartbeat(
        &self,
        grant: &LockGrant,
        actor: &ActorContext,
    ) -> Result<LockGrant, StoreError> {
        let mut connection = self.store.connection()?;
        let transaction =
            connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let record = match load_lock(&transaction, grant.task_id)? {
            Some(record) => record,
            None => {
                drop(transaction);
                return self.reject_grant(
                    grant,
                    actor,
                    "task.lock.heartbeat",
                    "stale_lock",
                    StoreError::StaleLock,
                );
            }
        };
        if let Err(error) = validate_grant(&record, grant, self.clock.now_millis()) {
            drop(transaction);
            return self.reject_grant(
                grant,
                actor,
                "task.lock.heartbeat",
                error_code(&error),
                error,
            );
        }

        let now = self.clock.now_millis();
        let expires_at = self.expiry_from(now)?;
        transaction.execute(
            "UPDATE task_locks SET heartbeat_at = ?1, expires_at = ?2
             WHERE task_id = ?3 AND state = 'active' AND fencing_generation = ?4",
            params![now, expires_at, grant.task_id.get(), grant.fencing_generation],
        )?;
        insert_lock_history(
            &transaction,
            grant.task_id,
            actor,
            "task.lock.heartbeat",
            now,
            grant.fencing_generation,
            None,
        )?;
        transaction.commit()?;

        Ok(LockGrant {
            task_id: grant.task_id,
            owner_id: grant.owner_id.clone(),
            token: grant.token.clone(),
            fencing_generation: grant.fencing_generation,
            acquired_at: grant.acquired_at,
            heartbeat_at: now,
            expires_at,
        })
    }

    pub fn release(
        &self,
        grant: &LockGrant,
        actor: &ActorContext,
    ) -> Result<(), StoreError> {
        let mut connection = self.store.connection()?;
        let transaction =
            connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let record = match load_lock(&transaction, grant.task_id)? {
            Some(record) => record,
            None => {
                drop(transaction);
                return self.reject_grant(
                    grant,
                    actor,
                    "task.lock.release",
                    "stale_lock",
                    StoreError::StaleLock,
                );
            }
        };
        if let Err(error) = validate_grant(&record, grant, self.clock.now_millis()) {
            drop(transaction);
            return self.reject_grant(
                grant,
                actor,
                "task.lock.release",
                error_code(&error),
                error,
            );
        }

        let now = self.clock.now_millis();
        transaction.execute(
            "UPDATE task_locks
             SET state = 'released', owner_id = NULL, token_hash = NULL,
                 heartbeat_at = ?1, expires_at = NULL, released_at = ?1
             WHERE task_id = ?2 AND state = 'active' AND fencing_generation = ?3",
            params![now, grant.task_id.get(), grant.fencing_generation],
        )?;
        insert_lock_history(
            &transaction,
            grant.task_id,
            actor,
            "task.lock.release",
            now,
            grant.fencing_generation,
            None,
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn recover(
        &self,
        task_id: TaskId,
        new_owner_id: &str,
        actor: &ActorContext,
    ) -> Result<LockGrant, StoreError> {
        if let Err(error) = validate_owner_id(new_owner_id) {
            return self.reject(
                Some(task_id),
                actor,
                "task.lock.recover",
                "invalid_request",
                Some(&serde_json::json!({"task_id": task_id}).to_string()),
                error,
            );
        }

        let mut connection = self.store.connection()?;
        let transaction =
            connection.transaction_with_behavior(TransactionBehavior::Exclusive)?;
        let record = match load_lock(&transaction, task_id)? {
            Some(record) => record,
            None => {
                drop(transaction);
                return self.reject(
                    Some(task_id),
                    actor,
                    "task.lock.recover",
                    "lock_expired",
                    None,
                    StoreError::LockExpired,
                );
            }
        };
        if record.state != "active" {
            drop(transaction);
            return self.reject(
                Some(task_id),
                actor,
                "task.lock.recover",
                "lock_expired",
                None,
                StoreError::LockExpired,
            );
        }

        let now = self.clock.now_millis();
        let expires_at = record.expires_at.ok_or_else(|| {
            StoreError::InvalidMetadata("active lock has no expiry".into())
        })?;
        if now < expires_at {
            drop(transaction);
            return self.reject(
                Some(task_id),
                actor,
                "task.lock.recover",
                "task_locked",
                None,
                StoreError::TaskLocked,
            );
        }
        let grace_ms = seconds_to_millis(self.config.locking.recovery_grace_seconds)?;
        if now < expires_at.saturating_add(grace_ms) {
            drop(transaction);
            return self.reject(
                Some(task_id),
                actor,
                "task.lock.recover",
                "lock_expired",
                None,
                StoreError::LockExpired,
            );
        }

        let generation = record.fencing_generation.checked_add(1).ok_or_else(|| {
            StoreError::InvalidMetadata("fencing_generation overflow".into())
        })?;
        let token = generate_token()?;
        let token_hash = token_hash(&token);
        let new_expiry = self.expiry_from(now)?;
        transaction.execute(
            "UPDATE task_locks
             SET state = 'active', owner_id = ?1, token_hash = ?2,
                 acquired_at = ?3, heartbeat_at = ?3, expires_at = ?4,
                 released_at = NULL, fencing_generation = ?5
             WHERE task_id = ?6 AND state = 'active' AND fencing_generation = ?7",
            params![
                new_owner_id,
                token_hash,
                now,
                new_expiry,
                generation,
                task_id.get(),
                record.fencing_generation
            ],
        )?;
        insert_lock_history(
            &transaction,
            task_id,
            actor,
            "task.lock.recover",
            now,
            generation,
            None,
        )?;
        transaction.commit()?;

        Ok(LockGrant {
            task_id,
            owner_id: new_owner_id.to_owned(),
            token,
            fencing_generation: generation,
            acquired_at: now,
            heartbeat_at: now,
            expires_at: new_expiry,
        })
    }

    fn expiry_from(&self, now: i64) -> Result<i64, StoreError> {
        let lease_ms = seconds_to_millis(self.config.locking.lease_seconds)?;
        now.checked_add(lease_ms)
            .ok_or_else(|| StoreError::InvalidConfig("lock expiry overflows epoch milliseconds".into()))
    }

    fn reject<T>(
        &self,
        task_id: Option<TaskId>,
        actor: &ActorContext,
        operation: &str,
        code: &str,
        requested_json: Option<&str>,
        error: StoreError,
    ) -> Result<T, StoreError> {
        record_rejected(&self.store, task_id, actor, operation, code, requested_json)?;
        Err(error)
    }

    fn reject_grant<T>(
        &self,
        grant: &LockGrant,
        actor: &ActorContext,
        operation: &str,
        code: &str,
        error: StoreError,
    ) -> Result<T, StoreError> {
        let details = serde_json::json!({
            "owner_id": grant.owner_id,
            "fencing_generation": grant.fencing_generation
        }).to_string();
        self.reject(
            Some(grant.task_id),
            actor,
            operation,
            code,
            Some(&details),
            error,
        )
    }
}

pub(crate) fn check_lock_access(
    connection: &Connection,
    task_id: TaskId,
    credential: Option<&LockCredential>,
    now_millis: i64,
) -> Result<Option<i64>, StoreError> {
    let Some(record) = load_lock(connection, task_id)? else {
        return if credential.is_some() {
            Err(StoreError::StaleLock)
        } else {
            Ok(None)
        };
    };

    match record.state.as_str() {
        "released" => {
            if credential.is_some() {
                Err(StoreError::StaleLock)
            } else {
                Ok(None)
            }
        }
        "active" => {
            let Some(credential) = credential else {
                return Err(StoreError::TaskLocked);
            };
            let owner_matches = record.owner_id.as_deref() == Some(credential.owner_id.as_str());
            let generation_matches = record.fencing_generation == credential.fencing_generation;
            let token_matches = record
                .token_hash
                .as_deref()
                .is_some_and(|stored| constant_time_equal(stored, &token_hash(&credential.token)));
            if !(owner_matches && generation_matches && token_matches) {
                return Err(StoreError::StaleLock);
            }
            if record.expires_at.is_none_or(|expiry| expiry <= now_millis) {
                return Err(StoreError::LockExpired);
            }
            Ok(Some(record.fencing_generation))
        }
        _ => Err(StoreError::InvalidMetadata("unknown task lock state".into())),
    }
}

fn load_lock(
    connection: &Connection,
    task_id: TaskId,
) -> Result<Option<LockRecord>, StoreError> {
    connection
        .query_row(
            "SELECT state, owner_id, token_hash, expires_at, fencing_generation
             FROM task_locks WHERE task_id = ?1",
            params![task_id.get()],
            |row| {
                Ok(LockRecord {
                    state: row.get(0)?,
                    owner_id: row.get(1)?,
                    token_hash: row.get(2)?,
                    expires_at: row.get(3)?,
                    fencing_generation: row.get(4)?,
                })
            },
        )
        .optional()
        .map_err(StoreError::from)
}

fn validate_grant(
    record: &LockRecord,
    grant: &LockGrant,
    now_millis: i64,
) -> Result<(), StoreError> {
    if record.state != "active"
        || record.owner_id.as_deref() != Some(grant.owner_id.as_str())
        || record.fencing_generation != grant.fencing_generation
        || !record
            .token_hash
            .as_deref()
            .is_some_and(|stored| constant_time_equal(stored, &token_hash(&grant.token)))
    {
        return Err(StoreError::StaleLock);
    }
    if record.expires_at.is_none_or(|expiry| expiry <= now_millis) {
        return Err(StoreError::LockExpired);
    }
    Ok(())
}

fn insert_lock_history(
    transaction: &Transaction<'_>,
    task_id: TaskId,
    actor: &ActorContext,
    operation: &str,
    now: i64,
    generation: i64,
    details_json: Option<&str>,
) -> Result<(), StoreError> {
    transaction.execute(
        "INSERT INTO operation_history(
            task_id, actor_id, occurred_at, operation, outcome,
            details_json, fencing_generation
         ) VALUES (?1, ?2, ?3, ?4, 'succeeded', ?5, ?6)",
        params![
            task_id.get(),
            actor.actor_id().as_str(),
            timestamp(now),
            operation,
            details_json,
            generation
        ],
    )?;
    Ok(())
}

fn record_rejected(
    store: &Store,
    task_id: Option<TaskId>,
    actor: &ActorContext,
    operation: &str,
    code: &str,
    requested_json: Option<&str>,
) -> Result<(), StoreError> {
    let mut connection = store.connection()?;
    let transaction =
        connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    transaction.execute(
        "INSERT INTO operation_history(
            task_id, actor_id, occurred_at, operation, requested_json,
            outcome, error_code
         ) VALUES (?1, ?2, ?3, ?4, ?5, 'rejected', ?6)",
        params![
            task_id.map(TaskId::get),
            actor.actor_id().as_str(),
            timestamp(SystemClock.now_millis()),
            operation,
            requested_json,
            code
        ],
    ).map_err(|error| StoreError::HistoryRecordFailed(error.to_string()))?;
    transaction
        .commit()
        .map_err(|error| StoreError::HistoryRecordFailed(error.to_string()))
}

fn validate_owner_id(owner_id: &str) -> Result<(), StoreError> {
    if owner_id.trim().is_empty()
        || owner_id.trim() != owner_id
        || owner_id.chars().any(char::is_control)
        || owner_id.chars().count() > 128
    {
        return Err(StoreError::InvalidTask(
            "Lock owner ID must contain 1 to 128 non-whitespace characters".into(),
        ));
    }
    Ok(())
}

fn generate_token() -> Result<String, StoreError> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|error| StoreError::Io(std::io::Error::other(error.to_string())))?;
    Ok(hex::encode(bytes))
}

fn token_hash(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (a, b)| difference | (a ^ b))
        == 0
}

fn seconds_to_millis(seconds: u64) -> Result<i64, StoreError> {
    let millis = seconds
        .checked_mul(1000)
        .ok_or_else(|| StoreError::InvalidConfig("lock duration overflows milliseconds".into()))?;
    i64::try_from(millis)
        .map_err(|_| StoreError::InvalidConfig("lock duration exceeds supported range".into()))
}

fn timestamp(epoch_millis: i64) -> String {
    let seconds = epoch_millis.div_euclid(1000);
    let millis = epoch_millis.rem_euclid(1000) as u32;
    chrono::DateTime::from_timestamp(seconds, millis * 1_000_000)
        .unwrap_or_else(Utc::now)
        .to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn error_code(error: &StoreError) -> &'static str {
    match error {
        StoreError::TaskLocked => "task_locked",
        StoreError::LockExpired => "lock_expired",
        _ => "stale_lock",
    }
}
