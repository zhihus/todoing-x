use rusqlite::{Connection, OptionalExtension, params};
use std::{
    env,
    error::Error,
    fs,
    path::Path,
    process,
    thread,
    time::Duration,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn open(path: &str) -> Result<Connection> {
    let connection = Connection::open(path)?;
    connection.busy_timeout(Duration::from_secs(10))?;
    Ok(connection)
}

fn unix_time() -> Result<i64> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs() as i64)
}

fn init(path: &str) -> Result<()> {
    if let Some(parent) = Path::new(path).parent() {
        fs::create_dir_all(parent)?;
    }
    let connection = open(path)?;
    connection.execute_batch(
        "PRAGMA journal_mode=DELETE;
         PRAGMA synchronous=FULL;
         PRAGMA foreign_keys=ON;
         CREATE TABLE tasks (
             id INTEGER PRIMARY KEY,
             title TEXT NOT NULL
         );
         CREATE TABLE task_id_seq (
             id INTEGER PRIMARY KEY CHECK (id = 1),
             next_id INTEGER NOT NULL
         );
         CREATE TABLE task_locks (
             task_id INTEGER PRIMARY KEY REFERENCES tasks(id),
             owner TEXT NOT NULL,
             expires_at INTEGER NOT NULL,
             fencing_token INTEGER NOT NULL
         );
         INSERT INTO tasks(id, title) VALUES (1, 'initial');
         INSERT INTO task_id_seq(id, next_id) VALUES (1, 2);"
    )?;
    Ok(())
}

fn hold_read(path: &str, ready: &str, millis: u64) -> Result<()> {
    let connection = open(path)?;
    connection.execute_batch("BEGIN DEFERRED;")?;
    let _: i64 = connection.query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get(0))?;
    fs::write(ready, b"ready")?;
    thread::sleep(Duration::from_millis(millis));
    connection.execute_batch("COMMIT;")?;
    Ok(())
}

fn acquire(path: &str, owner: &str) -> Result<()> {
    let mut connection = open(path)?;
    let transaction = connection.transaction_with_behavior(rusqlite::TransactionBehavior::Exclusive)?;
    let expiry = unix_time()? + 3600;
    transaction.execute(
        "INSERT INTO task_locks(task_id, owner, expires_at, fencing_token)
         VALUES (1, ?1, ?2, 1)",
        params![owner, expiry],
    )?;
    transaction.commit()?;
    Ok(())
}

fn recover(path: &str, owner: &str) -> Result<()> {
    let mut connection = open(path)?;
    let transaction = connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let expiry = unix_time()? + 3600;
    let changed = transaction.execute(
        "UPDATE task_locks
         SET owner = ?1, expires_at = ?2, fencing_token = fencing_token + 1
         WHERE task_id = 1 AND expires_at <= ?3",
        params![owner, expiry, unix_time()?],
    )?;
    if changed != 1 {
        return Err("task lock is not expired or does not exist".into());
    }
    transaction.commit()?;
    Ok(())
}

fn expire(path: &str) -> Result<()> {
    let connection = open(path)?;
    connection.execute("UPDATE task_locks SET expires_at = 0 WHERE task_id = 1", [])?;
    Ok(())
}

fn check_read(path: &str, owner: &str) -> Result<()> {
    let mut connection = open(path)?;
    let transaction = connection.transaction()?;
    let lock: Option<(String, i64)> = transaction
        .query_row(
            "SELECT owner, expires_at FROM task_locks WHERE task_id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    if let Some((lock_owner, expires_at)) = lock {
        if lock_owner != owner || expires_at <= unix_time()? {
            transaction.rollback()?;
            process::exit(20);
        }
    }
    let _: String = transaction.query_row("SELECT title FROM tasks WHERE id = 1", [], |row| row.get(0))?;
    transaction.commit()?;
    Ok(())
}

fn create(path: &str, title: &str) -> Result<i64> {
    let mut connection = open(path)?;
    let transaction = connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let id: i64 = transaction.query_row(
        "SELECT next_id FROM task_id_seq WHERE id = 1",
        [],
        |row| row.get(0),
    )?;
    transaction.execute("UPDATE task_id_seq SET next_id = ?1 WHERE id = 1", [id + 1])?;
    transaction.execute("INSERT INTO tasks(id, title) VALUES (?1, ?2)", params![id, title])?;
    transaction.commit()?;
    Ok(id)
}

fn crash_write(path: &str, ready: &str) -> Result<()> {
    let mut connection = open(path)?;
    let transaction = connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    transaction.execute("INSERT INTO tasks(id, title) VALUES (999, 'uncommitted')", [])?;
    fs::write(ready, b"uncommitted")?;
    process::exit(77);
}

fn check_absent(path: &str) -> Result<()> {
    let connection = open(path)?;
    let count: i64 = connection.query_row("SELECT COUNT(*) FROM tasks WHERE id = 999", [], |row| row.get(0))?;
    if count != 0 {
        return Err("uncommitted row survived process termination".into());
    }
    Ok(())
}

fn mutate(path: &str, owner: &str, token: i64) -> Result<()> {
    let mut connection = open(path)?;
    let transaction = connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let lock: Option<(String, i64, i64)> = transaction
        .query_row(
            "SELECT owner, fencing_token, expires_at FROM task_locks WHERE task_id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    match lock {
        Some((lock_owner, generation, expires_at))
            if lock_owner == owner && generation == token && expires_at > unix_time()? => {}
        _ => {
            transaction.rollback()?;
            process::exit(21);
        }
    }
    transaction.execute("UPDATE tasks SET title = ?1 WHERE id = 1", [format!("updated by {owner}")])?;
    transaction.commit()?;
    Ok(())
}

fn run() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let command = args.get(1).map(String::as_str).unwrap_or("");
    match command {
        "init" => init(&args[2])?,
        "hold-read" => hold_read(&args[2], &args[3], args[4].parse()?)?,
        "acquire" => acquire(&args[2], &args[3])?,
        "recover" => recover(&args[2], &args[3])?,
        "expire" => expire(&args[2])?,
        "read" => check_read(&args[2], &args[3])?,
        "create" => println!("{}", create(&args[2], &args[3])?),
        "crash-write" => crash_write(&args[2], &args[3])?,
        "check-absent" => check_absent(&args[2])?,
        "mutate" => mutate(&args[2], &args[3], args[4].parse()?)?,
        _ => return Err(format!("unknown or incomplete command: {command}").into()),
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        process::exit(1);
    }
}
