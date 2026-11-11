use serde_json::Value;
use std::{fs, path::Path, process::Command};
use tempfile::tempdir;

fn run(root: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_todoing"))
        .current_dir(root)
        .args(args)
        .output()
        .expect("todoing binary should run")
}

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .expect("git should be installed");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn json_interface_covers_task_reads_and_mutations() {
    let dir = tempdir().unwrap();

    let added = run(dir.path(), &["add", "json task", "--json"]);
    assert!(added.status.success());
    let task: Value = serde_json::from_slice(&added.stdout).unwrap();
    assert_eq!(task["id"], 1);
    assert_eq!(task["title"], "json task");

    let listed = run(dir.path(), &["ls", "--json"]);
    assert!(listed.status.success());
    let tasks: Value = serde_json::from_slice(&listed.stdout).unwrap();
    assert_eq!(tasks.as_array().unwrap().len(), 1);

    let changed = run(dir.path(), &["status", "1", "in_progress", "--json"]);
    assert!(changed.status.success());
    let task: Value = serde_json::from_slice(&changed.stdout).unwrap();
    assert_eq!(task["status"], "in_progress");

    let shown = run(dir.path(), &["see", "1", "--json"]);
    assert!(shown.status.success());
    let task: Value = serde_json::from_slice(&shown.stdout).unwrap();
    assert_eq!(task["title"], "json task");
}

#[test]
fn completion_requires_git_commit_and_all_active_openspec_tasks() {
    let dir = tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["config", "user.email", "todoing-test@example.invalid"]);
    git(dir.path(), &["config", "user.name", "Todoing Test"]);
    fs::write(dir.path().join("tracked.txt"), "initial").unwrap();
    git(dir.path(), &["add", "tracked.txt"]);
    git(dir.path(), &["commit", "-m", "initial"]);

    assert!(run(dir.path(), &["add", "finish feature"]).status.success());
    assert!(!run(dir.path(), &["status", "1", "done"]).status.success());

    assert!(run(dir.path(), &["commit", "1", "HEAD"]).status.success());
    let tasks_dir = dir.path().join("openspec/changes/example");
    fs::create_dir_all(&tasks_dir).unwrap();
    let tasks_file = tasks_dir.join("tasks.md");
    fs::write(&tasks_file, "- [ ] Finish implementation\n").unwrap();
    assert!(!run(dir.path(), &["status", "1", "done"]).status.success());

    fs::write(&tasks_file, "- [x] Finish implementation\n").unwrap();
    assert!(run(dir.path(), &["status", "1", "done"]).status.success());
}

#[cfg(unix)]
#[test]
fn on_create_hook_can_reject_task_creation() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    let hooks = dir.path().join(".todoing/hooks");
    fs::create_dir_all(&hooks).unwrap();
    let hook = hooks.join("on-create");
    fs::write(&hook, "#!/bin/sh\nexit 7\n").unwrap();
    let mut permissions = fs::metadata(&hook).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&hook, permissions).unwrap();

    let rejected = run(dir.path(), &["add", "must be rejected"]);
    assert!(!rejected.status.success());
    assert!(!dir.path().join(".todoing/tasks/1.md").exists());
}
