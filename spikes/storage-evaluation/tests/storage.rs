use std::{
    fs::OpenOptions,
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use tempfile::tempdir;
use todoing_storage_evaluation::{Counts, Fault, RedbStore, SqliteStore};

fn worker() -> &'static str {
    env!("CARGO_BIN_EXE_storage-worker")
}

fn initialize(backend: &str, path: &Path) {
    match backend {
        "sqlite" => SqliteStore::initialize(path).unwrap(),
        "redb" => RedbStore::initialize(path).unwrap(),
        _ => panic!("unknown backend"),
    }
}

fn counts(backend: &str, path: &Path) -> Counts {
    match backend {
        "sqlite" => SqliteStore::open(path).unwrap().counts().unwrap(),
        "redb" => RedbStore::open(path).unwrap().counts().unwrap(),
        _ => panic!("unknown backend"),
    }
}

fn create_with_failure(backend: &str, path: &Path) {
    match backend {
        "sqlite" => {
            let mut store = SqliteStore::open(path).unwrap();
            assert!(
                store
                    .create_task_with_fault("will-rollback", Fault::ReturnAfterTask)
                    .is_err()
            );
        }
        "redb" => {
            let store = RedbStore::open(path).unwrap();
            assert!(
                store
                    .create_task_with_fault("will-rollback", Fault::ReturnAfterTask)
                    .is_err()
            );
        }
        _ => panic!("unknown backend"),
    }
}

#[test]
fn sqlite_transaction_rolls_back_task_version_and_history_together() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("todoing.sqlite");
    initialize("sqlite", &path);

    create_with_failure("sqlite", &path);
    assert_eq!(counts("sqlite", &path), Counts::default());

    let mut store = SqliteStore::open(&path).unwrap();
    store.create_task("committed").unwrap();
    assert_eq!(
        store.counts().unwrap(),
        Counts {
            tasks: 1,
            versions: 1,
            history: 1
        }
    );
}

#[test]
fn redb_transaction_rolls_back_task_version_and_history_together() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("todoing.redb");
    initialize("redb", &path);

    create_with_failure("redb", &path);
    assert_eq!(counts("redb", &path), Counts::default());

    let store = RedbStore::open(&path).unwrap();
    store.create_task("committed").unwrap();
    assert_eq!(
        store.counts().unwrap(),
        Counts {
            tasks: 1,
            versions: 1,
            history: 1
        }
    );
}

fn assert_multi_process_writes(backend: &str, path: &Path) {
    initialize(backend, path);
    let mut children = Vec::new();

    for worker_id in 0..4 {
        let child = Command::new(worker())
            .arg("write")
            .arg(backend)
            .arg(path)
            .arg("25")
            .arg(worker_id.to_string())
            .spawn()
            .unwrap();
        children.push(child);
    }

    for mut child in children {
        let status = child.wait().unwrap();
        assert!(status.success(), "{backend} worker failed: {status}");
    }

    assert_eq!(
        counts(backend, path),
        Counts {
            tasks: 100,
            versions: 100,
            history: 100
        }
    );
}

#[test]
fn sqlite_serializes_concurrent_process_writes_without_lost_records() {
    let dir = tempdir().unwrap();
    assert_multi_process_writes("sqlite", &dir.path().join("todoing.sqlite"));
}

#[test]
fn redb_serializes_concurrent_process_writes_without_lost_records() {
    let dir = tempdir().unwrap();
    assert_multi_process_writes("redb", &dir.path().join("todoing.redb"));
}

fn assert_process_exit_recovery(backend: &str, path: &Path) {
    initialize(backend, path);

    let status = Command::new(worker())
        .arg("crash")
        .arg(backend)
        .arg(path)
        .status()
        .unwrap();
    assert_eq!(
        status.code(),
        Some(86),
        "{backend} worker did not exit at the fault point"
    );

    assert_eq!(counts(backend, path), Counts::default());
}

#[test]
fn sqlite_recovers_after_process_exit_before_commit() {
    let dir = tempdir().unwrap();
    assert_process_exit_recovery("sqlite", &dir.path().join("todoing.sqlite"));
}

#[test]
fn redb_recovers_after_process_exit_before_commit() {
    let dir = tempdir().unwrap();
    assert_process_exit_recovery("redb", &dir.path().join("todoing.redb"));
}

#[test]
fn shared_file_access_guard_blocks_exclusive_lock_until_operation_finishes() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("task-1.lock");

    let mut child = Command::new(worker())
        .arg("guard")
        .arg("file")
        .arg(&path)
        .arg("700")
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);
    let mut ready = String::new();
    reader.read_line(&mut ready).unwrap();
    assert_eq!(ready.trim(), "ready");

    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    let started = Instant::now();
    file.lock().unwrap();
    let elapsed = started.elapsed();
    file.unlock().unwrap();
    drop(file);

    let status = child.wait().unwrap();
    assert!(status.success());
    assert!(
        elapsed >= Duration::from_millis(400),
        "exclusive lock did not wait for the active shared operation: {elapsed:?}"
    );
}
