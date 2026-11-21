use redb::{
    Builder, ConcurrencyMode, Database, ReadableDatabase, ReadableTable, ReadableTableMetadata,
    TableDefinition,
};
use rusqlite::{params, Connection, TransactionBehavior};
use serde::Serialize;
use std::{path::Path, time::Duration};

const META: TableDefinition<&str, u64> = TableDefinition::new("meta");
const TASKS: TableDefinition<u64, &[u8]> = TableDefinition::new("tasks");
const VERSIONS: TableDefinition<u64, &[u8]> = TableDefinition::new("task_versions");
const HISTORY: TableDefinition<u64, &[u8]> = TableDefinition::new("operation_history");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    None,
    ReturnAfterTask,
    ExitAfterTask,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub tasks: u64,
    pub versions: u64,
    pub history: u64,
}

#[derive(Serialize)]
struct TaskRecord<'a> {
    id: u64,
    title: &'a str,
}

#[derive(Serialize)]
struct HistoryRecord<'a> {
    task_id: u64,
    actor: &'a str,
    operation: &'a str,
    outcome: &'a str,
}

pub struct SqliteStore {
    connection: Connection,
}

impl SqliteStore {
    pub fn initialize(path: impl AsRef<Path>) -> Result<(), String> {
        let connection = Connection::open(path).map_err(|e| e.to_string())?;
        connection
            .pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| e.to_string())?;
        connection
            .execute_batch(
                "PRAGMA foreign_keys = ON;
                 CREATE TABLE IF NOT EXISTS tasks (
                     id INTEGER PRIMARY KEY AUTOINCREMENT,
                     title TEXT NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS task_versions (
                     task_id INTEGER PRIMARY KEY REFERENCES tasks(id),
                     snapshot TEXT NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS operation_history (
                     task_id INTEGER PRIMARY KEY REFERENCES tasks(id),
                     actor TEXT NOT NULL,
                     operation TEXT NOT NULL,
                     outcome TEXT NOT NULL
                 );",
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, String> {
        let connection = Connection::open(path).map_err(|e| e.to_string())?;
        connection
            .busy_timeout(Duration::from_secs(30))
            .map_err(|e| e.to_string())?;
        connection
            .pragma_update(None, "foreign_keys", true)
            .map_err(|e| e.to_string())?;
        Ok(Self { connection })
    }

    pub fn create_task(&mut self, title: &str) -> Result<u64, String> {
        self.create_task_with_fault(title, Fault::None)
    }

    pub fn create_task_with_fault(
        &mut self,
        title: &str,
        fault: Fault,
    ) -> Result<u64, String> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;

        transaction
            .execute("INSERT INTO tasks (title) VALUES (?1)", [title])
            .map_err(|e| e.to_string())?;
        let id = transaction.last_insert_rowid() as u64;

        match fault {
            Fault::None => {}
            Fault::ReturnAfterTask => return Err("injected failure after task insert".into()),
            Fault::ExitAfterTask => std::process::exit(86),
        }

        let snapshot = serde_json::to_string(&TaskRecord { id, title })
            .map_err(|e| e.to_string())?;
        transaction
            .execute(
                "INSERT INTO task_versions (task_id, snapshot) VALUES (?1, ?2)",
                params![id as i64, snapshot],
            )
            .map_err(|e| e.to_string())?;
        transaction
            .execute(
                "INSERT INTO operation_history (task_id, actor, operation, outcome)
                 VALUES (?1, 'storage-spike', 'create', 'success')",
                [id as i64],
            )
            .map_err(|e| e.to_string())?;
        transaction.commit().map_err(|e| e.to_string())?;
        Ok(id)
    }

    pub fn counts(&self) -> Result<Counts, String> {
        let count = |table: &str| -> Result<u64, String> {
            let sql = format!("SELECT COUNT(*) FROM {table}");
            self.connection
                .query_row(&sql, [], |row| row.get::<_, i64>(0))
                .map(|value| value as u64)
                .map_err(|e| e.to_string())
        };
        Ok(Counts {
            tasks: count("tasks")?,
            versions: count("task_versions")?,
            history: count("operation_history")?,
        })
    }
}

pub struct RedbStore {
    database: Database,
}

impl RedbStore {
    pub fn initialize(path: impl AsRef<Path>) -> Result<(), String> {
        drop(Self::open(path)?);
        Ok(())
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, String> {
        let mut builder = Builder::new();
        builder.set_concurrency_mode(ConcurrencyMode::MultiWriter);
        let database = builder.create(path).map_err(|e| e.to_string())?;
        Ok(Self { database })
    }

    pub fn create_task(&self, title: &str) -> Result<u64, String> {
        self.create_task_with_fault(title, Fault::None)
    }

    pub fn create_task_with_fault(
        &self,
        title: &str,
        fault: Fault,
    ) -> Result<u64, String> {
        let transaction = self.database.begin_write().map_err(|e| e.to_string())?;

        let id = {
            let mut meta = transaction.open_table(META).map_err(|e| e.to_string())?;
            let next_id = meta
                .get("next_task_id")
                .map_err(|e| e.to_string())?
                .map(|value| *value.value())
                .unwrap_or(1);
            meta.insert("next_task_id", &(next_id + 1))
                .map_err(|e| e.to_string())?;
            next_id
        };

        let snapshot = serde_json::to_vec(&TaskRecord { id, title })
            .map_err(|e| e.to_string())?;
        {
            let mut tasks = transaction.open_table(TASKS).map_err(|e| e.to_string())?;
            tasks
                .insert(&id, snapshot.as_slice())
                .map_err(|e| e.to_string())?;
        }

        match fault {
            Fault::None => {}
            Fault::ReturnAfterTask => return Err("injected failure after task insert".into()),
            Fault::ExitAfterTask => std::process::exit(86),
        }

        let history = serde_json::to_vec(&HistoryRecord {
            task_id: id,
            actor: "storage-spike",
            operation: "create",
            outcome: "success",
        })
        .map_err(|e| e.to_string())?;
        {
            let mut versions = transaction.open_table(VERSIONS).map_err(|e| e.to_string())?;
            versions
                .insert(&id, snapshot.as_slice())
                .map_err(|e| e.to_string())?;
        }
        {
            let mut history_table = transaction.open_table(HISTORY).map_err(|e| e.to_string())?;
            history_table
                .insert(&id, history.as_slice())
                .map_err(|e| e.to_string())?;
        }

        transaction.commit().map_err(|e| e.to_string())?;
        Ok(id)
    }

    pub fn counts(&self) -> Result<Counts, String> {
        let transaction = self.database.begin_read().map_err(|e| e.to_string())?;
        let tasks = transaction.open_table(TASKS).map_err(|e| e.to_string())?;
        let versions = transaction.open_table(VERSIONS).map_err(|e| e.to_string())?;
        let history = transaction.open_table(HISTORY).map_err(|e| e.to_string())?;
        Ok(Counts {
            tasks: tasks.len().map_err(|e| e.to_string())?,
            versions: versions.len().map_err(|e| e.to_string())?,
            history: history.len().map_err(|e| e.to_string())?,
        })
    }
}
