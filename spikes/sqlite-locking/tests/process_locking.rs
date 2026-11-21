use std::{
    fs,
    path::Path,
    process::{Child, Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_sqlite-lock-spike")
}

fn run(args: &[&str]) -> Output {
    Command::new(binary()).args(args).output().expect("run spike binary")
}

fn assert_success(output: Output) -> String {
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn wait_for_file(path: &Path, child: &mut Child) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if path.exists() {
            return;
        }
        if let Some(status) = child.try_wait().expect("poll child") {
            panic!("child exited before ready signal: {status}");
        }
        thread::sleep(Duration::from_millis(10));
    }
    let _ = child.kill();
    panic!("timed out waiting for child ready signal");
}

#[test]
fn exclusive_lock_acquisition_waits_for_existing_reader_and_then_excludes_non_owner() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("todoing.db");
    let ready = dir.path().join("reader.ready");
    assert_success(run(&["init", db.to_str().unwrap()]));

    let mut reader = Command::new(binary())
        .args(["hold-read", db.to_str().unwrap(), ready.to_str().unwrap(), "600"])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    wait_for_file(&ready, &mut reader);

    let started = Instant::now();
    assert_success(run(&["acquire", db.to_str().unwrap(), "agent-a", "secret-a"]));
    let elapsed = started.elapsed();
    assert!(elapsed >= Duration::from_millis(300), "exclusive acquisition did not wait for the active reader: {elapsed:?}");
    assert!(reader.wait().unwrap().success());

    let denied = run(&["read", db.to_str().unwrap(), "agent-b", "secret-b"]);
    assert_eq!(denied.status.code(), Some(20));
    assert_success(run(&["read", db.to_str().unwrap(), "agent-a", "secret-a"]));
}

#[test]
fn concurrent_processes_allocate_unique_task_ids() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("todoing.db");
    assert_success(run(&["init", db.to_str().unwrap()]));

    let mut children = Vec::new();
    for index in 0..8 {
        children.push(
            Command::new(binary())
                .args(["create", db.to_str().unwrap(), &format!("task-{index}")])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        );
    }

    let mut ids = Vec::new();
    for child in children {
        let output = child.wait_with_output().unwrap();
        ids.push(assert_success(output).parse::<i64>().unwrap());
    }
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), 8, "concurrent clients allocated duplicate IDs");
}

#[test]
fn uncommitted_transaction_is_rolled_back_after_process_termination() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("todoing.db");
    let ready = dir.path().join("uncommitted.ready");
    assert_success(run(&["init", db.to_str().unwrap()]));

    let mut writer = Command::new(binary())
        .args(["crash-write", db.to_str().unwrap(), ready.to_str().unwrap()])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    wait_for_file(&ready, &mut writer);
    let status = writer.wait().unwrap();
    assert_eq!(status.code(), Some(77));
    assert_success(run(&["check-absent", db.to_str().unwrap()]));
}

#[test]
fn fencing_generation_rejects_a_former_lock_owner() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("todoing.db");
    assert_success(run(&["init", db.to_str().unwrap()]));
    assert_success(run(&["acquire", db.to_str().unwrap(), "agent-a", "secret-a"]));
    assert_success(run(&["expire", db.to_str().unwrap()]));
    assert_success(run(&["recover", db.to_str().unwrap(), "agent-b", "secret-b"]));

    let stale = run(&["mutate", db.to_str().unwrap(), "agent-a", "secret-a", "1"]);
    assert_eq!(stale.status.code(), Some(21));
    assert_success(run(&["mutate", db.to_str().unwrap(), "agent-b", "secret-b", "2"]));
}

#[test]
fn database_is_created_at_requested_path() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("nested").join("todoing.db");
    assert_success(run(&["init", db.to_str().unwrap()]));
    assert!(db.is_file());
    assert!(fs::metadata(db).unwrap().len() > 0);
}
