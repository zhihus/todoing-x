use serde_json::Value;
use std::{fs, path::Path, process::Command};
use std::io::Write;
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
fn before_done_hook_can_reject_transition_without_mutating_task() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["config", "user.email", "todoing-test@example.invalid"]);
    git(dir.path(), &["config", "user.name", "Todoing Test"]);
    fs::write(dir.path().join("tracked.txt"), "initial").unwrap();
    git(dir.path(), &["add", "tracked.txt"]);
    git(dir.path(), &["commit", "-m", "initial"]);

    assert!(run(dir.path(), &["add", "guarded task", "-s", "in_progress"]).status.success());
    assert!(run(dir.path(), &["commit", "1", "HEAD"]).status.success());

    let hooks = dir.path().join(".todoing/hooks");
    fs::create_dir_all(&hooks).unwrap();
    let hook = hooks.join("before-done");
    fs::write(&hook, "#!/bin/sh\ncat >/dev/null\nexit 9\n").unwrap();
    let mut permissions = fs::metadata(&hook).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&hook, permissions).unwrap();

    let rejected = run(dir.path(), &["status", "1", "done"]);
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("before-done hook rejected"));

    let shown = run(dir.path(), &["see", "1", "--json"]);
    let task: Value = serde_json::from_slice(&shown.stdout).unwrap();
    assert_eq!(task["status"], "in_progress");
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


fn run_with_input(root: &Path, args: &[&str], input: &str) -> std::process::Output {
    use std::process::Stdio;
    let mut child = Command::new(env!("CARGO_BIN_EXE_todoing"))
        .current_dir(root)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("todoing binary should run");
    child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn legacy_cli_keeps_original_output_order_and_filters() {
    let dir = tempdir().unwrap();
    assert!(run(dir.path(), &["add", "first", "-l", "bug", "-l", "api"]).status.success());
    assert!(run(dir.path(), &["add", "second", "-s", "in_progress"]).status.success());
    assert!(run(dir.path(), &["add", "third", "-l", "bug", "-s", "done"]).status.success());

    let listed = run(dir.path(), &["ls"]);
    let output = String::from_utf8_lossy(&listed.stdout);
    let lines: Vec<_> = output.lines().collect();
    assert_eq!(lines.len(), 3);
    assert!(lines[0].contains("third"));
    assert!(lines[1].contains("second"));
    assert!(lines[2].contains("first"));

    let filtered = run(dir.path(), &["ls", "-s", "done", "-l", "BUG"]);
    assert_eq!(String::from_utf8_lossy(&filtered.stdout).lines().count(), 1);
    assert!(String::from_utf8_lossy(&filtered.stdout).contains("third"));

    let searched = run(dir.path(), &["ls", "-q", "API"]);
    assert!(String::from_utf8_lossy(&searched.stdout).contains("first"));

    let reversed = run(dir.path(), &["ls", "-r", "-n", "2"]);
    let reversed = String::from_utf8_lossy(&reversed.stdout);
    assert!(reversed.lines().next().unwrap().contains("first"));
}

#[cfg(unix)]
#[test]
fn edit_opens_existing_task_file_even_if_frontmatter_is_malformed() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    let tasks = dir.path().join(".todoing/tasks");
    fs::create_dir_all(&tasks).unwrap();
    fs::write(tasks.join("1.md"), "malformed existing task").unwrap();

    let editor = dir.path().join("editor.sh");
    fs::write(&editor, "#!/bin/sh\nexit 0\n").unwrap();
    let mut permissions = fs::metadata(&editor).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&editor, permissions).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_todoing"))
        .current_dir(dir.path())
        .env("EDITOR", &editor)
        .args(["edit", "1"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn negative_list_limit_is_treated_as_unlimited() {
    let dir = tempdir().unwrap();
    for title in ["one", "two", "three"] {
        assert!(run(dir.path(), &["add", title]).status.success());
    }
    let listed = run(dir.path(), &["ls", "-n", "-1"]);
    assert!(listed.status.success());
    assert_eq!(String::from_utf8_lossy(&listed.stdout).lines().count(), 3);
}

#[test]
fn scan_rebuilds_index_from_task_markdown_files() {
    let dir = tempdir().unwrap();
    assert!(run(dir.path(), &["add", "indexed task"]).status.success());
    fs::write(
        dir.path().join(".todoing/tasks/2.md"),
        "---\nadded_at: '2026-05-01T10:00:00'\nid: 2\nlabels: []\nstatus: todo\ntitle: scanned task\n---\n",
    ).unwrap();

    let before = run(dir.path(), &["ls"]);
    assert!(!String::from_utf8_lossy(&before.stdout).contains("scanned task"));

    let scanned = run(dir.path(), &["ls", "--scan"]);
    assert!(scanned.status.success());
    let output = String::from_utf8_lossy(&scanned.stdout);
    assert!(output.contains("scanned task"));
    assert!(output.contains("indexed task"));
}

#[test]
fn query_searches_body_labels_and_status() {
    let dir = tempdir().unwrap();
    assert!(run(dir.path(), &["add", "alpha", "-b", "needle in body"]).status.success());
    assert!(run(dir.path(), &["add", "beta", "-l", "special-label"]).status.success());
    assert!(run(dir.path(), &["add", "gamma", "-s", "in_progress"]).status.success());

    for query in ["needle", "special-label", "in_progress"] {
        let result = run(dir.path(), &["ls", "-q", query]);
        assert!(result.status.success());
        assert_eq!(String::from_utf8_lossy(&result.stdout).lines().count(), 1, "query: {query}");
    }
}

#[test]
fn combined_query_and_label_filters_use_and_semantics() {
    let dir = tempdir().unwrap();
    assert!(run(dir.path(), &["add", "auth bug", "-l", "bug", "-s", "in_progress"]).status.success());
    assert!(run(dir.path(), &["add", "ui bug", "-l", "bug"]).status.success());
    let result = run(dir.path(), &["ls", "-q", "auth", "-l", "bug"]);
    let output = String::from_utf8_lossy(&result.stdout);
    assert!(result.status.success());
    assert!(output.contains("auth bug"));
    assert!(!output.contains("ui bug"));
}

#[test]
fn add_with_body_and_labels_is_readable() {
    let dir = tempdir().unwrap();
    let added = run(dir.path(), &["add", "documented", "-l", "urgent", "-l", "p0", "-b", "details"]);
    assert!(added.status.success());
    assert!(String::from_utf8_lossy(&added.stdout).contains("(#1) documented"));

    let shown = run(dir.path(), &["see", "1"]);
    let output = String::from_utf8_lossy(&shown.stdout);
    assert!(output.contains("details"));
    assert!(output.contains("labels: [urgent, p0]"));
}

#[test]
fn body_append_and_label_operations_persist() {
    let dir = tempdir().unwrap();
    assert!(run(dir.path(), &["add", "mutable", "-l", "old", "-b", "line one"]).status.success());
    assert!(run(dir.path(), &["body", "1", "replacement"]).status.success());
    assert!(run(dir.path(), &["append", "1", "second paragraph"]).status.success());
    assert!(run(dir.path(), &["label", "1", "+new", "--", "-old"]).status.success());

    let shown = run(dir.path(), &["see", "1"]);
    let text = String::from_utf8_lossy(&shown.stdout);
    assert!(text.contains("replacement\n\nsecond paragraph"));
    assert!(text.contains("labels: [new]"));
    assert!(!text.contains("line one"));
}

#[test]
fn task_creation_rejects_blank_required_title_without_writing_files() {
    let dir = tempdir().unwrap();
    for title in ["", "   ", "\t\n"] {
        let rejected = run(dir.path(), &["add", title]);
        assert!(!rejected.status.success());
        assert!(String::from_utf8_lossy(&rejected.stderr).contains("Task title cannot be empty"));
    }
    let empty_label = run(dir.path(), &["add", "valid title", "-l", "   "]);
    assert!(!empty_label.status.success());
    assert!(String::from_utf8_lossy(&empty_label.stderr).contains("Task labels cannot be empty"));
    assert!(!dir.path().join(".todoing/tasks/1.md").exists());
}

#[test]
fn label_no_clear_option_preserves_existing_labels() {
    let dir = tempdir().unwrap();
    assert!(run(dir.path(), &["add", "labels", "-l", "existing"]).status.success());
    let changed = run(dir.path(), &["label", "1", "+new", "--no-clear"]);
    assert!(changed.status.success());
    let shown = run(dir.path(), &["see", "1"]);
    let text = String::from_utf8_lossy(&shown.stdout);
    assert!(text.contains("labels: [existing, new]"));
}

#[test]
fn invalid_status_and_identifier_are_rejected() {
    let dir = tempdir().unwrap();
    let bad_status = run(dir.path(), &["add", "invalid", "-s", "blocked"]);
    assert!(!bad_status.status.success());
    assert!(String::from_utf8_lossy(&bad_status.stderr).contains("Invalid status"));

    let bad_id = run(dir.path(), &["see", "+1"]);
    assert!(!bad_id.status.success());
    assert!(String::from_utf8_lossy(&bad_id.stderr).contains("Invalid task ID"));
}

#[test]
fn delete_confirmation_treats_eof_as_abort_error() {
    let dir = tempdir().unwrap();
    assert!(run(dir.path(), &["add", "keep me"]).status.success());

    let output = run_with_input(dir.path(), &["delete", "1"], "");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Aborted!"));
    assert!(String::from_utf8_lossy(&run(dir.path(), &["ls"]).stdout).contains("keep me"));
}

#[test]
fn delete_confirmation_preserves_or_removes_task() {
    let dir = tempdir().unwrap();
    assert!(run(dir.path(), &["add", "keep me"]).status.success());

    let refused = run_with_input(dir.path(), &["delete", "1"], "n\n");
    assert!(refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stdout).contains("Delete (#1) keep me? [y/N]: "));
    assert!(String::from_utf8_lossy(&refused.stdout).contains("Aborted."));
    assert!(String::from_utf8_lossy(&run(dir.path(), &["ls"]).stdout).contains("keep me"));

    let accepted = run_with_input(dir.path(), &["delete", "1"], "yes\n");
    assert!(accepted.status.success());
    assert!(String::from_utf8_lossy(&run(dir.path(), &["ls"]).stdout).contains("No tasks."));
}

#[test]
fn reindex_recovers_missing_index_from_markdown() {
    let dir = tempdir().unwrap();
    assert!(run(dir.path(), &["add", "recover me"]).status.success());
    fs::remove_file(dir.path().join(".todoing/index.json")).unwrap();

    let rebuilt = run(dir.path(), &["reindex"]);
    assert!(rebuilt.status.success());
    assert!(String::from_utf8_lossy(&rebuilt.stdout).contains("1 tasks"));
    assert!(String::from_utf8_lossy(&run(dir.path(), &["ls"]).stdout).contains("recover me"));
}

#[test]
fn special_title_and_multiline_body_round_trip() {
    let dir = tempdir().unwrap();
    assert!(run(dir.path(), &["add", "123", "-b", "line one\nline two"]).status.success());
    let shown = run(dir.path(), &["see", "#1"]);
    let text = String::from_utf8_lossy(&shown.stdout);
    assert!(text.contains("title: '123'") || text.contains("title: \"123\""));
    assert!(text.contains("line one\nline two"));
}
