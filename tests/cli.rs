use serde_json::Value;
use std::{fs, path::Path, process::Command};
use std::io::Write;
use tempfile::tempdir;

fn run(root: &Path, args: &[&str]) -> std::process::Output {
    let binary = Path::new(env!("CARGO_BIN_EXE_todoing"));
    let mut paths = vec![binary.parent().unwrap().to_path_buf()];
    paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()));
    let path = std::env::join_paths(paths).unwrap();
    Command::new(binary)
        .current_dir(root)
        .env("PATH", path)
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
    assert!(version["data"]["version"].is_string());
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

#[test]
fn reindex_restores_commit_refs_after_python_style_index_rewrite() {
    let dir = tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["config", "user.email", "todoing-test@example.invalid"]);
    git(dir.path(), &["config", "user.name", "Todoing Test"]);
    fs::write(dir.path().join("tracked.txt"), "initial").unwrap();
    git(dir.path(), &["add", "tracked.txt"]);
    git(dir.path(), &["commit", "-m", "initial"]);

    assert!(run(dir.path(), &["add", "linked task"]).status.success());
    assert!(run(dir.path(), &["commit", "1", "HEAD"]).status.success());
    let markdown = fs::read_to_string(dir.path().join(".todoing/tasks/1.md")).unwrap();
    assert!(markdown.contains("commits: ["));

    // Python's Pydantic model ignores the Rust-only field when it rewrites index.json.
    fs::write(
        dir.path().join(".todoing/index.json"),
        r#"[{"id":1,"title":"linked task","added_at":"2026-05-01T10:00:00","status":"todo","labels":[],"body":""}]"#,
    ).unwrap();

    let listed = run(dir.path(), &["ls", "--json"]);
    assert!(listed.status.success());
    let tasks: Value = serde_json::from_slice(&listed.stdout).unwrap();
    assert!(tasks[0].get("commits").and_then(Value::as_array).is_none_or(Vec::is_empty));

    let rebuilt = run(dir.path(), &["reindex", "--json"]);
    assert!(rebuilt.status.success(), "{}", String::from_utf8_lossy(&rebuilt.stderr));
    let tasks: Value = serde_json::from_slice(&rebuilt.stdout).unwrap();
    assert_eq!(tasks[0]["commits"].as_array().unwrap().len(), 1);
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
    let installed = run(dir.path(), &["hooks", "install", "all"]);
    assert!(installed.status.success(), "{}", String::from_utf8_lossy(&installed.stderr));
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["config", "user.email", "todoing-test@example.invalid"]);
    git(dir.path(), &["config", "user.name", "Todoing Test"]);
    let direct_done = run(dir.path(), &["add", "direct done", "--status", "done"]);
    assert!(!direct_done.status.success());
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
    let archived = dir.path().join("openspec/changes/archive/old-change");
    fs::create_dir_all(&archived).unwrap();
    fs::write(archived.join("tasks.md"), "- [ ] archived task\n").unwrap();
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
fn missing_completion_hooks_allow_done_without_linked_commits() {
    let dir = tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    assert!(run(dir.path(), &["add", "task without commit"]).status.success());

    let completed = run(dir.path(), &["status", "1", "done"]);
    assert!(completed.status.success(), "{}", String::from_utf8_lossy(&completed.stderr));

    let shown = run(dir.path(), &["see", "1", "--json"]);
    let task: Value = serde_json::from_slice(&shown.stdout).unwrap();
    assert_eq!(task["status"], "done");
}
#[test]
fn missing_openspec_hook_does_not_block_done() {
    let dir = tempdir().unwrap();
    assert!(run(dir.path(), &["add", "task with open checklist"]).status.success());

    let changes = dir.path().join("openspec/changes");
    fs::create_dir_all(changes.join("active")).unwrap();
    fs::write(changes.join("active/tasks.md"), "- [ ] unfinished task\n").unwrap();

    let completed = run(dir.path(), &["status", "1", "done"]);
    assert!(completed.status.success(), "{}", String::from_utf8_lossy(&completed.stderr));
}
#[test]
fn standard_hooks_can_be_installed_selectively_and_status_is_machine_readable() {
    let dir = tempdir().unwrap();

    let status = run(dir.path(), &["hooks", "status", "--json"]);
    assert!(status.status.success());
    let value: Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(value["api_version"], 1);
    assert_eq!(value["data"]["hooks"][0]["name"], "before-done-git");
    assert_eq!(value["data"]["hooks"][0]["installed"], false);
    assert_eq!(value["data"]["hooks"][1]["installed"], false);
    assert_eq!(value["data"]["agent_action"], "ask_user_before_install");

    let installed = run(dir.path(), &["hooks", "install", "git", "--json"]);
    assert!(installed.status.success(), "{}", String::from_utf8_lossy(&installed.stderr));
    let value: Value = serde_json::from_slice(&installed.stdout).unwrap();
    assert_eq!(value["data"]["installed"][0], "before-done-git");

    #[cfg(windows)]
    let git_hook = dir.path().join(".todoing/hooks/before-done-git.cmd");
    #[cfg(not(windows))]
    let git_hook = dir.path().join(".todoing/hooks/before-done-git");
    #[cfg(windows)]
    let openspec_hook = dir.path().join(".todoing/hooks/before-done-openspec.cmd");
    #[cfg(not(windows))]
    let openspec_hook = dir.path().join(".todoing/hooks/before-done-openspec");
    assert!(git_hook.is_file());
    assert!(!openspec_hook.exists());

    fs::write(&git_hook, "user-customized hook").unwrap();
    let all = run(dir.path(), &["hooks", "install", "all"]);
    assert!(all.status.success(), "{}", String::from_utf8_lossy(&all.stderr));
    assert_eq!(fs::read_to_string(&git_hook).unwrap(), "user-customized hook");
    assert!(openspec_hook.is_file());
    let status = run(dir.path(), &["hooks", "status", "--json"]);
    let value: Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(value["data"]["hooks"][0]["installed"], true);
    assert_eq!(value["data"]["hooks"][1]["installed"], true);
    assert_eq!(value["data"]["agent_action"], "none");
}

#[cfg(unix)]
#[test]
fn before_done_git_hook_can_reject_completion() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    assert!(run(dir.path(), &["add", "git hook task"]).status.success());
    let hooks = dir.path().join(".todoing/hooks");
    fs::create_dir_all(&hooks).unwrap();
    let hook = hooks.join("before-done-git");
    fs::write(&hook, "#!/bin/sh\nexit 7\n").unwrap();
    let mut permissions = fs::metadata(&hook).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&hook, permissions).unwrap();

    let rejected = run(dir.path(), &["status", "1", "done"]);
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("before-done-git hook rejected"));
}

#[cfg(unix)]
#[test]
fn before_done_openspec_hook_can_reject_completion() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    assert!(run(dir.path(), &["add", "openspec hook task"]).status.success());
    let hooks = dir.path().join(".todoing/hooks");
    fs::create_dir_all(&hooks).unwrap();
    let hook = hooks.join("before-done-openspec");
    fs::write(&hook, "#!/bin/sh\nexit 7\n").unwrap();
    let mut permissions = fs::metadata(&hook).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&hook, permissions).unwrap();

    let rejected = run(dir.path(), &["status", "1", "done"]);
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("before-done-openspec hook rejected"));
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

#[cfg(windows)]
#[test]
fn before_done_cmd_hook_can_reject_transition_without_mutating_task() {
    let dir = tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["config", "user.email", "todoing-test@example.invalid"]);
    git(dir.path(), &["config", "user.name", "Todoing Test"]);
    fs::write(dir.path().join("tracked.txt"), "initial").unwrap();
    git(dir.path(), &["add", "tracked.txt"]);
    git(dir.path(), &["commit", "-m", "initial"]);

    let added = run(dir.path(), &["add", "guarded task", "-s", "in_progress"]);
    assert!(added.status.success());
    let linked = run(dir.path(), &["commit", "1", "HEAD"]);
    assert!(linked.status.success(), "{}", String::from_utf8_lossy(&linked.stderr));

    let hooks = dir.path().join(".todoing/hooks");
    fs::create_dir_all(&hooks).unwrap();
    fs::write(hooks.join("before-done.cmd"), "@echo off\r\nexit /b 9\r\n").unwrap();

    let rejected = run(dir.path(), &["status", "1", "done"]);
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("before-done hook rejected"));

    let shown = run(dir.path(), &["see", "1", "--json"]);
    let task: Value = serde_json::from_slice(&shown.stdout).unwrap();
    assert_eq!(task["status"], "in_progress");
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
fn init_creates_a_new_store_and_refuses_reinitialization() {
    let dir = tempdir().unwrap();
    let initialized = run(dir.path(), &["init", "--json"]);
    assert!(initialized.status.success(), "{}", String::from_utf8_lossy(&initialized.stderr));
    let response: Value = serde_json::from_slice(&initialized.stdout).unwrap();
    assert_eq!(response["ok"], true);
    assert!(response["data"]["project_id"].as_str().unwrap().starts_with("p-"));
    assert_eq!(response["data"]["schema_version"], 2);
    assert!(dir.path().join(".todoing/todoing.db").is_file());
    assert!(dir.path().join(".todoing/config.toml").is_file());

    let repeated = run(dir.path(), &["init", "--json"]);
    assert!(!repeated.status.success());
    let response: Value = serde_json::from_slice(&repeated.stdout).unwrap();
    assert_eq!(response["error"]["code"], "store_already_initialized");
}

#[test]
fn init_preserves_valid_config_and_rejects_invalid_config() {
    let dir = tempdir().unwrap();
    let config_dir = dir.path().join(".todoing");
    fs::create_dir_all(&config_dir).unwrap();
    let config_path = config_dir.join("config.toml");
    let source = "format_version = 1\n\n[[statuses]]\nid = \"todo\"\nlabel = \"To Do\"\nenabled = true\n";
    fs::write(&config_path, source).unwrap();
    let initialized = run(dir.path(), &["init", "--project-id", "custom-project"]);
    assert!(initialized.status.success(), "{}", String::from_utf8_lossy(&initialized.stderr));
    assert_eq!(fs::read_to_string(config_path).unwrap(), source);

    let invalid = tempdir().unwrap();
    fs::create_dir_all(invalid.path().join(".todoing")).unwrap();
    fs::write(invalid.path().join(".todoing/config.toml"), "not = [valid").unwrap();
    let rejected = run(invalid.path(), &["init", "--json"]);
    assert!(!rejected.status.success());
    let response: Value = serde_json::from_slice(&rejected.stdout).unwrap();
    assert_eq!(response["error"]["code"], "config_invalid");
    assert!(!invalid.path().join(".todoing/todoing.db").exists());
}

#[test]
fn json_api_store_init_creates_a_project_store() {
    let dir = tempdir().unwrap();
    let request = serde_json::json!({
        "api_version": 1,
        "request_id": "bootstrap-1",
        "project_id": "api-project",
        "actor_id": "manifest",
        "operation": "store.init",
        "args": {}
    });
    let output = run_with_input(dir.path(), &["api"], &request.to_string());
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["request_id"], "bootstrap-1");
    assert_eq!(response["data"]["project_id"], "api-project");
    assert_eq!(response["data"]["schema_version"], 2);

    let info = serde_json::json!({
        "api_version": 1,
        "project_id": "api-project",
        "actor_id": "manifest",
        "operation": "store.info",
        "args": {}
    });
    let output = run_with_input(dir.path(), &["api"], &info.to_string());
    assert!(output.status.success());
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["data"]["project_id"], "api-project");
}

#[test]
fn init_refuses_legacy_task_data_without_creating_a_database() {
    let dir = tempdir().unwrap();
    let tasks = dir.path().join(".todoing/tasks");
    fs::create_dir_all(&tasks).unwrap();
    fs::write(tasks.join("1.md"), "legacy task").unwrap();

    let rejected = run(dir.path(), &["init", "--json"]);
    assert!(!rejected.status.success());
    let response: Value = serde_json::from_slice(&rejected.stdout).unwrap();
    assert_eq!(response["error"]["code"], "migration_required");
    assert!(!dir.path().join(".todoing/todoing.db").exists());
}

#[test]
fn init_rejects_invalid_supplied_project_id() {
    let dir = tempdir().unwrap();
    let rejected = run(dir.path(), &["init", "--project-id", "  ", "--json"]);
    assert!(!rejected.status.success());
    let response: Value = serde_json::from_slice(&rejected.stdout).unwrap();
    assert_eq!(response["error"]["code"], "invalid_request");
    assert!(!dir.path().join(".todoing/todoing.db").exists());
}

#[test]
fn help_preserves_original_command_surface_and_adds_only_commit_extension() {
    let dir = tempdir().unwrap();
    let output = run(dir.path(), &["--help"]);
    assert!(output.status.success());
    let help = String::from_utf8_lossy(&output.stdout);
    for command in ["add", "ls", "see", "edit", "body", "append", "status", "label", "delete", "reindex", "version", "commit"] {
        assert!(help.lines().any(|line| line.trim_start().starts_with(command)), "missing {command} in help: {help}");
    }
    assert!(!help.lines().any(|line| line.trim_start().starts_with("search ")));
    assert!(help.contains("--json"));
}

#[test]
fn legacy_cli_keeps_original_output_order_and_filters() {
    let dir = tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["config", "user.email", "todoing-test@example.invalid"]);
    git(dir.path(), &["config", "user.name", "Todoing Test"]);
    fs::write(dir.path().join("tracked.txt"), "initial").unwrap();
    git(dir.path(), &["add", "tracked.txt"]);
    git(dir.path(), &["commit", "-m", "initial"]);
    assert!(run(dir.path(), &["add", "first", "-l", "bug", "-l", "api"]).status.success());
    assert!(run(dir.path(), &["add", "second", "-s", "in_progress"]).status.success());
    assert!(run(dir.path(), &["add", "third", "-l", "bug"]).status.success());
    assert!(run(dir.path(), &["commit", "3", "HEAD"]).status.success());
    assert!(run(dir.path(), &["status", "3", "done"]).status.success());

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
fn delete_is_rejected_and_preserves_all_tasks_commit_metadata() {
    let dir = tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["config", "user.email", "todoing-test@example.invalid"]);
    git(dir.path(), &["config", "user.name", "Todoing Test"]);
    fs::write(dir.path().join("tracked.txt"), "initial").unwrap();
    git(dir.path(), &["add", "tracked.txt"]);
    git(dir.path(), &["commit", "-m", "initial"]);

    assert!(run(dir.path(), &["add", "delete me"]).status.success());
    assert!(run(dir.path(), &["add", "keep me"]).status.success());
    assert!(run(dir.path(), &["commit", "1", "HEAD"]).status.success());
    assert!(run(dir.path(), &["commit", "2", "HEAD"]).status.success());
    let deleted = run(dir.path(), &["delete", "1", "--force"]);
    assert!(!deleted.status.success());
    assert!(String::from_utf8_lossy(&deleted.stderr).contains("Physical deletion is disabled"));

    let listed = run(dir.path(), &["ls", "--json"]);
    assert!(listed.status.success(), "{}", String::from_utf8_lossy(&listed.stderr));
    let tasks: Value = serde_json::from_slice(&listed.stdout).unwrap();
    assert_eq!(tasks.as_array().unwrap().len(), 2);
    assert_eq!(tasks[0]["id"], 2);
    assert_eq!(tasks[0]["commits"].as_array().unwrap().len(), 1);
    assert_eq!(tasks[1]["id"], 1);
    assert_eq!(tasks[1]["commits"].as_array().unwrap().len(), 1);

    let markdown = fs::read_to_string(dir.path().join(".todoing/tasks/2.md")).unwrap();
    assert!(markdown.contains("commits: ["));
}

#[test]
fn commit_metadata_survives_body_label_and_status_mutations() {
    let dir = tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["config", "user.email", "todoing-test@example.invalid"]);
    git(dir.path(), &["config", "user.name", "Todoing Test"]);
    fs::write(dir.path().join("tracked.txt"), "initial").unwrap();
    git(dir.path(), &["add", "tracked.txt"]);
    git(dir.path(), &["commit", "-m", "initial"]);

    assert!(run(dir.path(), &["add", "mutable linked task", "-b", "initial body"]).status.success());
    assert!(run(dir.path(), &["commit", "1", "HEAD"]).status.success());
    assert!(run(dir.path(), &["body", "1", "replacement body"]).status.success());
    assert!(run(dir.path(), &["append", "1", "appended body"]).status.success());
    assert!(run(dir.path(), &["label", "1", "+verified"]).status.success());
    assert!(run(dir.path(), &["status", "1", "in_progress"]).status.success());

    let shown = run(dir.path(), &["see", "1", "--json"]);
    assert!(shown.status.success(), "{}", String::from_utf8_lossy(&shown.stderr));
    let task: Value = serde_json::from_slice(&shown.stdout).unwrap();
    assert_eq!(task["body"], "replacement body\n\nappended body");
    assert_eq!(task["labels"], serde_json::json!(["verified"]));
    assert_eq!(task["status"], "in_progress");
    assert_eq!(task["commits"].as_array().unwrap().len(), 1);

    let markdown = fs::read_to_string(dir.path().join(".todoing/tasks/1.md")).unwrap();
    assert!(markdown.contains("commits: ["));
}

#[test]
fn creating_done_task_runs_without_optional_completion_hooks() {
    let dir = tempdir().unwrap();
    let created = run(dir.path(), &["add", "directly done", "--status", "done"]);
    assert!(created.status.success(), "{}", String::from_utf8_lossy(&created.stderr));
    assert!(dir.path().join(".todoing/tasks/1.md").exists());
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
fn delete_always_rejects_physical_deletion_without_reading_stdin() {
    let dir = tempdir().unwrap();
    assert!(run(dir.path(), &["add", "keep me"]).status.success());

    let output = run_with_input(dir.path(), &["delete", "1"], "");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Physical deletion is disabled"));
    assert!(String::from_utf8_lossy(&run(dir.path(), &["ls"]).stdout).contains("keep me"));
}

#[test]
fn delete_rejection_preserves_task_regardless_of_confirmation_input() {
    let dir = tempdir().unwrap();
    assert!(run(dir.path(), &["add", "keep me"]).status.success());

    let refused = run_with_input(dir.path(), &["delete", "1"], "n\n");
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("Physical deletion is disabled"));
    assert!(String::from_utf8_lossy(&run(dir.path(), &["ls"]).stdout).contains("keep me"));

    let accepted = run_with_input(dir.path(), &["delete", "1"], "yes\n");
    assert!(!accepted.status.success());
    assert!(String::from_utf8_lossy(&accepted.stderr).contains("Physical deletion is disabled"));
    assert!(String::from_utf8_lossy(&run(dir.path(), &["ls"]).stdout).contains("keep me"));
}

#[test]
fn delete_json_returns_versioned_error_and_preserves_task() {
    let dir = tempdir().unwrap();
    assert!(run(dir.path(), &["add", "json delete"]).status.success());

    let refused = run_with_input(dir.path(), &["delete", "1", "--json"], "n\n");
    assert!(!refused.status.success());
    let result: Value = serde_json::from_slice(&refused.stdout).unwrap();
    assert_eq!(result["api_version"], 1);
    assert_eq!(result["ok"], false);
    assert_eq!(result["error"]["code"], "invalid_request");
    assert!(String::from_utf8_lossy(&refused.stderr).contains("Physical deletion is disabled"));
    assert!(String::from_utf8_lossy(&run(dir.path(), &["ls"]).stdout).contains("json delete"));
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


#[test]
fn legacy_cli_preserves_missing_task_errors_and_version_output() {
    let dir = tempdir().unwrap();

    for args in [
        vec!["see", "99"],
        vec!["body", "99", "text"],
        vec!["append", "99", "text"],
        vec!["status", "99", "done"],
        vec!["label", "99", "+label"],
    ] {
        let result = run(dir.path(), &args);
        assert!(!result.status.success(), "command unexpectedly succeeded: {args:?}");
        assert!(
            String::from_utf8_lossy(&result.stderr).contains("not found"),
            "unexpected error for {args:?}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let delete = run(dir.path(), &["delete", "99", "--force"]);
    assert!(!delete.status.success());
    assert!(String::from_utf8_lossy(&delete.stderr).contains("Physical deletion is disabled"));

    let version = run(dir.path(), &["version", "--json"]);
    assert!(version.status.success());
    let version: Value = serde_json::from_slice(&version.stdout).unwrap();
    assert_eq!(version["api_version"], 1);
    assert_eq!(version["data"]["version"], env!("CARGO_PKG_VERSION"));
}

#[test]
fn legacy_cli_accepts_every_original_status_value() {
    let dir = tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["config", "user.email", "todoing-test@example.invalid"]);
    git(dir.path(), &["config", "user.name", "Todoing Test"]);
    fs::write(dir.path().join("tracked.txt"), "initial").unwrap();
    git(dir.path(), &["add", "tracked.txt"]);
    git(dir.path(), &["commit", "-m", "initial"]);

    let added = run(dir.path(), &["add", "status compatibility"]);
    assert!(added.status.success());

    for status in ["todo", "in_progress", "cancelled"] {
        let changed = run(dir.path(), &["status", "1", status]);
        assert!(
            changed.status.success(),
            "status {status} rejected: {}",
            String::from_utf8_lossy(&changed.stderr)
        );
    }

    let linked = run(dir.path(), &["commit", "1", "HEAD"]);
    assert!(linked.status.success(), "{}", String::from_utf8_lossy(&linked.stderr));
    let done = run(dir.path(), &["status", "1", "done"]);
    assert!(done.status.success(), "{}", String::from_utf8_lossy(&done.stderr));

    let shown = run(dir.path(), &["see", "1", "--json"]);
    assert!(shown.status.success());
    let task: Value = serde_json::from_slice(&shown.stdout).unwrap();
    assert_eq!(task["status"], "done");
}
#[cfg(unix)]
#[test]
fn add_done_runs_on_create_after_optional_completion_hooks() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    let hooks = dir.path().join(".todoing/hooks");
    fs::create_dir_all(&hooks).unwrap();
    let marker = dir.path().join("hook-ran");
    let hook = hooks.join("on-create");
    fs::write(&hook, format!("#!/bin/sh\ntouch '{}'\n", marker.display())).unwrap();
    let mut permissions = fs::metadata(&hook).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&hook, permissions).unwrap();

    let created = run(dir.path(), &["add", "directly done", "--status", "done"]);
    assert!(created.status.success(), "{}", String::from_utf8_lossy(&created.stderr));
    assert!(marker.exists());
}
#[cfg(windows)]
#[test]
fn add_done_cmd_runs_on_create_after_optional_completion_hooks() {
    let dir = tempdir().unwrap();
    let hooks = dir.path().join(".todoing/hooks");
    fs::create_dir_all(&hooks).unwrap();
    let marker = dir.path().join("hook-ran");
    let hook = hooks.join("on-create.cmd");
    fs::write(&hook, format!("@echo off\necho ran > \"{}\"\r\n", marker.display())).unwrap();

    let created = run(dir.path(), &["add", "directly done", "--status", "done"]);
    assert!(created.status.success(), "{}", String::from_utf8_lossy(&created.stderr));
    assert!(marker.exists());
}


#[test]
fn explicit_migration_switches_human_cli_to_sqlite_lifecycle() {
    let dir = tempdir().unwrap();
    assert!(run(dir.path(), &["add", "legacy task"]).status.success());

    let migrated = run(dir.path(), &["migrate", "--project-id", "cli-integration", "--json"]);
    assert!(migrated.status.success(), "{}", String::from_utf8_lossy(&migrated.stderr));
    let report: Value = serde_json::from_slice(&migrated.stdout).unwrap();
    assert_eq!(report["api_version"], 1);
    assert_eq!(report["data"]["activated"], true);
    assert_eq!(report["data"]["task_count"], 1);
    assert!(dir.path().join(".todoing/todoing.db").is_file());

    let listed = run(dir.path(), &["ls", "--json"]);
    assert!(listed.status.success(), "{}", String::from_utf8_lossy(&listed.stderr));
    let listed: Value = serde_json::from_slice(&listed.stdout).unwrap();
    assert_eq!(listed["api_version"], 1);
    assert_eq!(listed["data"][0]["id"], 1);
    assert_eq!(listed["data"][0]["title"], "legacy task");

    let added = run(dir.path(), &["add", "SQLite task", "--json"]);
    assert!(added.status.success(), "{}", String::from_utf8_lossy(&added.stderr));
    let added: Value = serde_json::from_slice(&added.stdout).unwrap();
    assert_eq!(added["data"]["id"], 2);

    let body = run(dir.path(), &["body", "2", "updated body", "--json"]);
    assert!(body.status.success(), "{}", String::from_utf8_lossy(&body.stderr));
    let body: Value = serde_json::from_slice(&body.stdout).unwrap();
    assert_eq!(body["data"]["body"], "updated body");

    let dependency = run(dir.path(), &["depends", "add", "2", "1", "--json"]);
    assert!(dependency.status.success(), "{}", String::from_utf8_lossy(&dependency.stderr));

    let completed = run(dir.path(), &["complete", "1", "--json"]);
    assert!(completed.status.success(), "{}", String::from_utf8_lossy(&completed.stderr));
    let completed: Value = serde_json::from_slice(&completed.stdout).unwrap();
    assert_eq!(completed["data"]["outcome"], "completed");

    let reopened = run(dir.path(), &["reopen", "1", "--json"]);
    assert!(reopened.status.success(), "{}", String::from_utf8_lossy(&reopened.stderr));
    let reopened: Value = serde_json::from_slice(&reopened.stdout).unwrap();
    assert_eq!(reopened["data"]["outcome"], Value::Null);

    let cancelled = run(dir.path(), &["cancel", "2", "--reason", "obsolete", "--json"]);
    assert!(cancelled.status.success(), "{}", String::from_utf8_lossy(&cancelled.stderr));
    let cancelled: Value = serde_json::from_slice(&cancelled.stdout).unwrap();
    assert_eq!(cancelled["data"]["outcome"], "cancelled");

    let deleted = run(dir.path(), &["delete", "2", "--json"]);
    assert!(!deleted.status.success());
    let deleted: Value = serde_json::from_slice(&deleted.stdout).unwrap();
    assert_eq!(deleted["ok"], false);
    assert_eq!(deleted["error"]["code"], "invalid_request");

    let history = run(dir.path(), &["history", "1", "--json"]);
    assert!(history.status.success(), "{}", String::from_utf8_lossy(&history.stderr));
    let history: Value = serde_json::from_slice(&history.stdout).unwrap();
    assert!(history["data"].as_array().unwrap().len() >= 3);

    let versions = run(dir.path(), &["version", "1", "--json"]);
    assert!(versions.status.success(), "{}", String::from_utf8_lossy(&versions.stderr));
    let versions: Value = serde_json::from_slice(&versions.stdout).unwrap();
    assert!(versions["data"].as_array().unwrap().len() >= 2);

    let verified = run(dir.path(), &["verify", "--json"]);
    assert!(verified.status.success(), "{}", String::from_utf8_lossy(&verified.stderr));
    let verified: Value = serde_json::from_slice(&verified.stdout).unwrap();
    assert_eq!(verified["data"]["integrity_check"], "ok");

    let snapshot = run(dir.path(), &["snapshot", "cli-backup.db", "--json"]);
    assert!(snapshot.status.success(), "{}", String::from_utf8_lossy(&snapshot.stderr));
    let snapshot: Value = serde_json::from_slice(&snapshot.stdout).unwrap();
    assert_eq!(snapshot["data"]["size_bytes"].as_u64().unwrap() > 0, true);
    assert!(dir.path().join("cli-backup.db").is_file());
}


#[test]
fn json_api_transport_covers_migration_snapshot_response_and_exit_contract() {
    let dir = tempdir().unwrap();
    assert!(run(dir.path(), &["add", "legacy API task"]).status.success());

    let dry_run_request = serde_json::json!({
        "api_version": 1,
        "request_id": "migration-dry-run",
        "project_id": "api-transport-project",
        "actor_id": "integration-test",
        "operation": "store.migrate",
        "args": {"dry_run": true}
    });
    let dry_run = run_with_input(
        dir.path(),
        &["api"],
        &serde_json::to_string(&dry_run_request).unwrap(),
    );
    assert!(dry_run.status.success(), "{}", String::from_utf8_lossy(&dry_run.stderr));
    let dry_run_output = String::from_utf8_lossy(&dry_run.stdout).into_owned();
    let dry_run_lines: Vec<_> = dry_run_output.lines().collect();
    assert_eq!(dry_run_lines.len(), 1);
    let dry_run_response: Value = serde_json::from_str(dry_run_lines[0]).unwrap();
    assert_eq!(dry_run_response["request_id"], "migration-dry-run");
    assert_eq!(dry_run_response["ok"], true);
    assert_eq!(dry_run_response["data"]["dry_run"], true);
    assert_eq!(dry_run_response["data"]["activated"], false);
    assert!(!dir.path().join(".todoing/todoing.db").exists());

    let migrate_request = serde_json::json!({
        "api_version": 1,
        "request_id": "migration-activate",
        "project_id": "api-transport-project",
        "actor_id": "integration-test",
        "operation": "store.migrate",
        "args": {}
    });
    let migrated = run_with_input(
        dir.path(),
        &["api"],
        &serde_json::to_string(&migrate_request).unwrap(),
    );
    assert!(migrated.status.success(), "{}", String::from_utf8_lossy(&migrated.stderr));
    let migrated_response: Value = serde_json::from_slice(&migrated.stdout).unwrap();
    assert_eq!(migrated_response["data"]["activated"], true);
    assert_eq!(migrated_response["data"]["task_count"], 1);

    let snapshot_request = serde_json::json!({
        "api_version": 1,
        "request_id": "snapshot",
        "project_id": "api-transport-project",
        "actor_id": "integration-test",
        "operation": "store.snapshot",
        "args": {"output_path": "api-backup.db"}
    });
    let snapshot = run_with_input(
        dir.path(),
        &["api"],
        &serde_json::to_string(&snapshot_request).unwrap(),
    );
    assert!(snapshot.status.success(), "{}", String::from_utf8_lossy(&snapshot.stderr));
    let snapshot_response: Value = serde_json::from_slice(&snapshot.stdout).unwrap();
    assert_eq!(snapshot_response["ok"], true);
    assert_eq!(snapshot_response["data"]["sha256"].as_str().unwrap().len(), 64);
    assert!(dir.path().join("api-backup.db").is_file());

    let info_request = serde_json::json!({
        "api_version": 1,
        "request_id": "info",
        "project_id": "api-transport-project",
        "actor_id": "integration-test",
        "operation": "store.info",
        "args": {}
    });
    let info = run_with_input(
        dir.path(),
        &["api"],
        &serde_json::to_string(&info_request).unwrap(),
    );
    assert!(info.status.success());
    let info_output = String::from_utf8_lossy(&info.stdout).into_owned();
    let info_lines: Vec<_> = info_output.lines().collect();
    assert_eq!(info_lines.len(), 1);
    let info_response: Value = serde_json::from_str(info_lines[0]).unwrap();
    assert_eq!(info_response["request_id"], "info");
    assert_eq!(info_response["data"]["task_count"], 1);
    assert!(info.stderr.is_empty());

    let unsupported = serde_json::json!({
        "api_version": 99,
        "request_id": "unsupported",
        "project_id": "api-transport-project",
        "actor_id": "integration-test",
        "operation": "store.info",
        "args": {}
    });
    let rejected = run_with_input(
        dir.path(),
        &["api"],
        &serde_json::to_string(&unsupported).unwrap(),
    );
    assert_eq!(rejected.status.code(), Some(2));
    let rejected_output = String::from_utf8_lossy(&rejected.stdout).into_owned();
    let rejected_lines: Vec<_> = rejected_output.lines().collect();
    assert_eq!(rejected_lines.len(), 1);
    let rejected_response: Value = serde_json::from_str(rejected_lines[0]).unwrap();
    assert_eq!(rejected_response["request_id"], "unsupported");
    assert_eq!(rejected_response["ok"], false);
    assert_eq!(rejected_response["error"]["code"], "unsupported_api_version");
    assert!(!rejected.stderr.is_empty());
}


#[test]
fn sqlite_cli_archive_restore_is_explicit_non_destructive_and_versioned() {
    let dir = tempdir().unwrap();
    assert!(run(dir.path(), &["add", "archive prerequisite"]).status.success());
    assert!(run(dir.path(), &["add", "active dependent"]).status.success());
    let migrated = run(dir.path(), &["migrate", "--project-id", "archive-cli-project", "--json"]);
    assert!(migrated.status.success(), "{}", String::from_utf8_lossy(&migrated.stderr));

    let dependency = run(dir.path(), &["depends", "add", "2", "1", "--json"]);
    assert!(dependency.status.success(), "{}", String::from_utf8_lossy(&dependency.stderr));

    let archived = run(dir.path(), &["archive", "1", "--json"]);
    assert!(archived.status.success(), "{}", String::from_utf8_lossy(&archived.stderr));
    let archived: Value = serde_json::from_slice(&archived.stdout).unwrap();
    assert_eq!(archived["data"]["no_op"], false);
    assert!(archived["data"]["task"]["archived_at"].is_string());
    assert_eq!(archived["data"]["task"]["archived_by"], "cli-user");

    let active_list = run(dir.path(), &["ls", "--json"]);
    assert!(active_list.status.success());
    let active_list: Value = serde_json::from_slice(&active_list.stdout).unwrap();
    assert_eq!(active_list["data"].as_array().unwrap().len(), 1);
    assert_eq!(active_list["data"][0]["id"], 2);

    let archive_list = run(dir.path(), &["ls", "--archived", "--json"]);
    assert!(archive_list.status.success());
    let archive_list: Value = serde_json::from_slice(&archive_list.stdout).unwrap();
    assert_eq!(archive_list["data"].as_array().unwrap().len(), 1);
    assert_eq!(archive_list["data"][0]["id"], 1);

    let read = run(dir.path(), &["see", "1", "--json"]);
    assert!(read.status.success());
    let read: Value = serde_json::from_slice(&read.stdout).unwrap();
    assert!(read["data"]["archived_at"].is_string());

    let blocked = run(dir.path(), &["body", "1", "not allowed", "--json"]);
    assert!(!blocked.status.success());
    let blocked: Value = serde_json::from_slice(&blocked.stdout).unwrap();
    assert_eq!(blocked["error"]["code"], "task_archived");

    let lock_blocked = run(dir.path(), &["lock", "acquire", "1", "--json"]);
    assert!(!lock_blocked.status.success());
    let lock_blocked: Value = serde_json::from_slice(&lock_blocked.stdout).unwrap();
    assert_eq!(lock_blocked["error"]["code"], "task_archived");

    let history_before = run(dir.path(), &["history", "1", "--json"]);
    let history_before: Value = serde_json::from_slice(&history_before.stdout).unwrap();
    let version_before = run(dir.path(), &["version", "1", "--json"]);
    let version_before: Value = serde_json::from_slice(&version_before.stdout).unwrap();
    let version_count = version_before["data"].as_array().unwrap().len();

    let repeated = run(dir.path(), &["archive", "1", "--json"]);
    assert!(repeated.status.success());
    let repeated: Value = serde_json::from_slice(&repeated.stdout).unwrap();
    assert_eq!(repeated["data"]["no_op"], true);
    let version_after = run(dir.path(), &["version", "1", "--json"]);
    let version_after: Value = serde_json::from_slice(&version_after.stdout).unwrap();
    assert_eq!(version_after["data"].as_array().unwrap().len(), version_count);

    let restored = run(dir.path(), &["restore", "1", "--json"]);
    assert!(restored.status.success(), "{}", String::from_utf8_lossy(&restored.stderr));
    let restored: Value = serde_json::from_slice(&restored.stdout).unwrap();
    assert_eq!(restored["data"]["no_op"], false);
    assert!(restored["data"]["task"]["archived_at"].is_null());

    let restored_list = run(dir.path(), &["ls", "--json"]);
    let restored_list: Value = serde_json::from_slice(&restored_list.stdout).unwrap();
    assert_eq!(restored_list["data"].as_array().unwrap().len(), 2);

    let history_after = run(dir.path(), &["history", "1", "--json"]);
    let history_after: Value = serde_json::from_slice(&history_after.stdout).unwrap();
    let events = history_after["data"].as_array().unwrap();
    assert!(events.len() > history_before["data"].as_array().unwrap().len());
    assert!(events.iter().any(|event| event["operation"] == "task.archive"));
    assert!(events.iter().any(|event| event["operation"] == "task.restore"));

    let dependencies = run(dir.path(), &["depends", "list", "2", "--json"]);
    assert!(dependencies.status.success());
    let dependencies: Value = serde_json::from_slice(&dependencies.stdout).unwrap();
    assert_eq!(dependencies["data"].as_array().unwrap().len(), 1);
}
