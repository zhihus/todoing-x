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
fn json_interface_covers_remaining_commands() {
    let dir = tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["config", "user.email", "todoing-test@example.invalid"]);
    git(dir.path(), &["config", "user.name", "Todoing Test"]);
    fs::write(dir.path().join("tracked.txt"), "initial").unwrap();
    git(dir.path(), &["add", "tracked.txt"]);
    git(dir.path(), &["commit", "-m", "initial"]);

    let added = run(dir.path(), &["--json", "add", "json workflow"]);
    assert!(added.status.success());
    let _: Value = serde_json::from_slice(&added.stdout).unwrap();

    let body = run(dir.path(), &["body", "1", "first paragraph", "--json"]);
    assert!(body.status.success());
    let task: Value = serde_json::from_slice(&body.stdout).unwrap();
    assert_eq!(task["body"], "first paragraph");

    let appended = run(dir.path(), &["append", "1", "second paragraph", "--json"]);
    assert!(appended.status.success());
    let task: Value = serde_json::from_slice(&appended.stdout).unwrap();
    assert_eq!(task["body"], "first paragraph\n\nsecond paragraph");

    let labeled = run(dir.path(), &["label", "1", "+agent", "--json"]);
    assert!(labeled.status.success());
    let task: Value = serde_json::from_slice(&labeled.stdout).unwrap();
    assert_eq!(task["labels"][0], "agent");

    let linked = run(dir.path(), &["commit", "1", "HEAD", "--json"]);
    assert!(linked.status.success());
    let task: Value = serde_json::from_slice(&linked.stdout).unwrap();
    assert_eq!(task["commits"].as_array().unwrap().len(), 1);

    let rebuilt = run(dir.path(), &["reindex", "--json"]);
    assert!(rebuilt.status.success());
    let tasks: Value = serde_json::from_slice(&rebuilt.stdout).unwrap();
    assert_eq!(tasks.as_array().unwrap().len(), 1);

    let version = run(dir.path(), &["version", "--json"]);
    assert!(version.status.success());
    let version: Value = serde_json::from_slice(&version.stdout).unwrap();
    assert!(version["version"].is_string());
}

#[test]
fn list_scan_rebuilds_index_from_markdown_files() {
    let dir = tempdir().unwrap();
    let tasks = dir.path().join(".todoing/tasks");
    fs::create_dir_all(&tasks).unwrap();
    fs::write(tasks.join("7.md"), "---\nid: 7\ntitle: scanned task\nstatus: todo\nlabels: []\n---\n").unwrap();

    let listed = run(dir.path(), &["ls", "--scan", "--json"]);
    assert!(listed.status.success(), "{}", String::from_utf8_lossy(&listed.stderr));
    let tasks: Value = serde_json::from_slice(&listed.stdout).unwrap();
    assert_eq!(tasks.as_array().unwrap().len(), 1);
    assert_eq!(tasks[0]["id"], 7);
    assert!(dir.path().join(".todoing/index.json").exists());
}

#[test]
fn status_arguments_are_case_insensitive_like_python() {
    let dir = tempdir().unwrap();
    let added = run(dir.path(), &["add", "case insensitive", "--status", "IN_PROGRESS"]);
    assert!(added.status.success(), "{}", String::from_utf8_lossy(&added.stderr));

    let changed = run(dir.path(), &["status", "1", "CANCELLED"]);
    assert!(changed.status.success(), "{}", String::from_utf8_lossy(&changed.stderr));
    let filtered = run(dir.path(), &["ls", "--status", "CANCELLED", "--json"]);
    assert!(filtered.status.success(), "{}", String::from_utf8_lossy(&filtered.stderr));
    let filtered_tasks: Value = serde_json::from_slice(&filtered.stdout).unwrap();
    assert_eq!(filtered_tasks.as_array().unwrap().len(), 1);
    let shown = run(dir.path(), &["see", "1", "--json"]);
    let task: Value = serde_json::from_slice(&shown.stdout).unwrap();
    assert_eq!(task["status"], "cancelled");
}

#[test]
fn commit_links_store_full_sha_and_deduplicate_resolved_refs() {
    let dir = tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["config", "user.email", "todoing-test@example.invalid"]);
    git(dir.path(), &["config", "user.name", "Todoing Test"]);
    fs::write(dir.path().join("tracked.txt"), "initial").unwrap();
    git(dir.path(), &["add", "tracked.txt"]);
    git(dir.path(), &["commit", "-m", "initial"]);

    let added = run(dir.path(), &["add", "linked task"]);
    assert!(added.status.success());
    let head = Command::new("git").current_dir(dir.path()).args(["rev-parse", "HEAD"])
        .output().unwrap();
    assert!(head.status.success());
    let full_sha = String::from_utf8(head.stdout).unwrap().trim().to_string();

    for reference in ["HEAD", "HEAD"] {
        let linked = run(dir.path(), &["commit", "1", reference, "--json"]);
        assert!(linked.status.success(), "{}", String::from_utf8_lossy(&linked.stderr));
        let task: Value = serde_json::from_slice(&linked.stdout).unwrap();
        assert_eq!(task["commits"].as_array().unwrap().len(), 1);
        assert_eq!(task["commits"][0], full_sha);
    }

    let rebuilt = run(dir.path(), &["reindex", "--json"]);
    assert!(rebuilt.status.success());
    let tasks: Value = serde_json::from_slice(&rebuilt.stdout).unwrap();
    assert_eq!(tasks[0]["commits"][0], full_sha);
}

#[cfg(unix)]
#[test]
fn edit_json_returns_updated_task_after_editor_and_reindex() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    assert!(run(dir.path(), &["add", "editable"]).status.success());
    let editor = dir.path().join("fake-editor.sh");
    fs::write(&editor, "#!/bin/sh\nexit 0\n").unwrap();
    let mut permissions = fs::metadata(&editor).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&editor, permissions).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_todoing"))
        .current_dir(dir.path())
        .env("EDITOR", &editor)
        .args(["edit", "1", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let task: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(task["id"], 1);
    assert_eq!(task["title"], "editable");
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

#[test]
fn commit_rejects_tree_object_without_mutating_task() {
    let dir = tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["config", "user.email", "todoing-test@example.invalid"]);
    git(dir.path(), &["config", "user.name", "Todoing Test"]);
    fs::write(dir.path().join("tracked.txt"), "initial").unwrap();
    git(dir.path(), &["add", "tracked.txt"]);
    git(dir.path(), &["commit", "-m", "initial"]);

    assert!(run(dir.path(), &["add", "commit validation"]).status.success());
    let tree = Command::new("git").current_dir(dir.path()).args(["rev-parse", "HEAD^{tree}"])
        .output().unwrap();
    assert!(tree.status.success());
    let tree_sha = String::from_utf8(tree.stdout).unwrap().trim().to_string();

    let rejected = run(dir.path(), &["commit", "1", &tree_sha]);
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("not a valid Git commit"));

    let shown = run(dir.path(), &["see", "1", "--json"]);
    let task: Value = serde_json::from_slice(&shown.stdout).unwrap();
    assert!(task.get("commits").is_none() || task["commits"].as_array().unwrap().is_empty());
}

#[test]
fn completion_rejects_unavailable_linked_commit_without_git_noise() {
    let dir = tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    assert!(run(dir.path(), &["add", "stale commit"]).status.success());

    let task_path = dir.path().join(".todoing/tasks/1.md");
    let contents = fs::read_to_string(&task_path).unwrap();
    let contents = contents.replacen("id: 1\n", "commits: [0000000000000000000000000000000000000000]\nid: 1\n", 1);
    fs::write(&task_path, contents).unwrap();

    let rejected = run(dir.path(), &["status", "1", "done"]);
    let stderr = String::from_utf8_lossy(&rejected.stderr);
    assert!(!rejected.status.success());
    assert!(stderr.contains("is not available in this repository"));
    assert!(!stderr.contains("fatal:"));

    let shown = run(dir.path(), &["see", "1", "--json"]);
    let task: Value = serde_json::from_slice(&shown.stdout).unwrap();
    assert_eq!(task["status"], "todo");
}

#[test]
fn completion_checks_every_active_change_and_ignores_archive() {
    let dir = tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["config", "user.email", "todoing-test@example.invalid"]);
    git(dir.path(), &["config", "user.name", "Todoing Test"]);
    fs::write(dir.path().join("tracked.txt"), "initial").unwrap();
    git(dir.path(), &["add", "tracked.txt"]);
    git(dir.path(), &["commit", "-m", "initial"]);

    assert!(run(dir.path(), &["add", "finish feature"]).status.success());
    assert!(run(dir.path(), &["commit", "1", "HEAD"]).status.success());

    let changes = dir.path().join("openspec/changes");
    fs::create_dir_all(changes.join("archive/old")).unwrap();
    fs::write(changes.join("archive/old/tasks.md"), "- [ ] archived task\n").unwrap();
    fs::create_dir_all(changes.join("active-one")).unwrap();
    fs::write(changes.join("active-one/tasks.md"), "- [x] completed task\n").unwrap();
    fs::create_dir_all(changes.join("active-two")).unwrap();
    let active_tasks = changes.join("active-two/tasks.md");
    fs::write(&active_tasks, "- [ ] unfinished task\n").unwrap();

    let rejected = run(dir.path(), &["status", "1", "done"]);
    assert!(!rejected.status.success());
    let error = String::from_utf8_lossy(&rejected.stderr);
    assert!(error.contains("active-two") && error.contains("tasks.md"));

    fs::write(&active_tasks, "- [X] completed task\n").unwrap();
    let accepted = run(dir.path(), &["status", "1", "done"]);
    assert!(accepted.status.success(), "{}", String::from_utf8_lossy(&accepted.stderr));
}

#[cfg(windows)]
#[test]
fn on_create_cmd_hook_can_reject_task_creation() {
    let dir = tempdir().unwrap();
    let hooks = dir.path().join(".todoing/hooks");
    fs::create_dir_all(&hooks).unwrap();
    fs::write(hooks.join("on-create.cmd"), "@echo off\r\nexit /b 9\r\n").unwrap();

    let rejected = run(dir.path(), &["add", "guarded task"]);
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("on-create hook rejected"));
    assert!(!dir.path().join(".todoing/tasks/1.md").exists());
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
fn delete_json_confirmation_keeps_stdout_machine_readable() {
    let dir = tempdir().unwrap();
    assert!(run(dir.path(), &["add", "json delete"]).status.success());

    let refused = run_with_input(dir.path(), &["delete", "1", "--json"], "n\n");
    assert!(refused.status.success());
    let result: Value = serde_json::from_slice(&refused.stdout).unwrap();
    assert_eq!(result["id"], 1);
    assert_eq!(result["deleted"], false);
    assert!(String::from_utf8_lossy(&refused.stderr).contains("Delete (#1) json delete?"));

    let accepted = run_with_input(dir.path(), &["delete", "1", "--json"], "yes\n");
    assert!(accepted.status.success());
    let result: Value = serde_json::from_slice(&accepted.stdout).unwrap();
    assert_eq!(result["deleted"], true);
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


#[test]
fn label_clear_flags_follow_last_option_like_click() {
    let dir = tempdir().unwrap();
    assert!(run(dir.path(), &["add", "labels", "-l", "existing"]).status.success());

    let clear_last = run(dir.path(), &["label", "1", "+new", "--no-clear", "--clear"]);
    assert!(clear_last.status.success(), "{}", String::from_utf8_lossy(&clear_last.stderr));
    let shown = run(dir.path(), &["see", "1"]);
    assert!(String::from_utf8_lossy(&shown.stdout).contains("labels: [new]"));

    let no_clear_last = run(dir.path(), &["label", "1", "+another", "--clear", "--no-clear"]);
    assert!(no_clear_last.status.success(), "{}", String::from_utf8_lossy(&no_clear_last.stderr));
    let shown = run(dir.path(), &["see", "1"]);
    let output = String::from_utf8_lossy(&shown.stdout);
    assert!(output.contains("labels: [new, another]"));
}
