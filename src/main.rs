use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    path::PathBuf,
    process::{Command, Stdio},
};
use tempfile::NamedTempFile;
use todoing::{
    config::ProjectConfig,
    domain::{ActorContext, ActorId, ProjectId, StatusId, TaskId, TaskOutcome},
    storage::{LockGrant, NewTask, Store as SqliteStore, StoreError, Task as SqliteTask, TaskRepository},
    migration,
    Store, Task as LegacyTask, TaskStatus, VERSION,
};

const GIT_HOOK_UNIX: &str = include_str!("../hooks/before-done-git");
const OPENSPEC_HOOK_UNIX: &str = include_str!("../hooks/before-done-openspec");
const GIT_HOOK_WINDOWS: &str = include_str!("../hooks/before-done-git.cmd");
const OPENSPEC_HOOK_WINDOWS: &str = include_str!("../hooks/before-done-openspec.cmd");

#[derive(Parser)]
#[command(name="todoing", version=VERSION, about="Local, git-friendly task manager for AI agents and humans", disable_help_subcommand=true, arg_required_else_help=true)]
struct Cli {
    #[arg(long, global=true, help="Emit machine-readable JSON")]
    json: bool,
    #[arg(long, global=true, help="Actor identity recorded in task history (or TODOING_ACTOR_ID)")]
    actor: Option<String>,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about="Create a new task.")]
    Add(AddArgs),
    #[command(name="ls")]
    #[command(about="List tasks, newest first.")]
    Ls(ListArgs),
    #[command(about="Show a task as markdown with frontmatter.")]
    See(SeeArgs),
    #[command(about="Open the task file in $EDITOR.")]
    Edit(EditArgs),
    #[command(about="Replace the description body of a task.")]
    Body(BodyArgs),
    #[command(about="Append text to the description body of a task.")]
    Append(AppendArgs),
    #[command(about="Change a task's status.")]
    Status(StatusArgs),
    #[command(about="Modify task labels with add/remove operations.")]
    Label(LabelArgs),
    #[command(about="Link a Git commit to a task.")]
    Commit(CommitArgs),
    #[command(about="Delete a task.")]
    Delete(DeleteArgs),
    #[command(about="Rebuild index.json from all task .md files.")]
    Reindex,
    #[command(about="Show the installed todoing version or task versions.")]
    Version(VersionArgs),
    #[command(about="Mark a task completed.")]
    Complete(TaskIdArgs),
    #[command(about="Archive a task without changing its workflow status.")]
    Archive(TaskIdArgs),
    #[command(about="Restore an archived task to the active task list.")]
    Restore(TaskIdArgs),
    #[command(about="Cancel a task without deleting it.")]
    Cancel(CancelArgs),
    #[command(about="Reopen a completed or cancelled task.")]
    Reopen(ReopenArgs),
    #[command(about="Manage task dependencies.")]
    Depends(DependsArgs),
    #[command(about="Show task operation history.")]
    History(HistoryArgs),
    #[command(about="Manage exclusive task locks.")]
    Lock(LockArgs),
    #[command(about="Show project configuration.")]
    Config,
    #[command(about="Initialize a new project-local SQLite task store.")]
    Init(InitArgs),
    #[command(about="Migrate legacy Markdown tasks to SQLite.")]
    Migrate(MigrateArgs),
    #[command(about="Verify the SQLite task store.")]
    Verify,
    #[command(about="Create a verified SQLite snapshot.")]
    Snapshot(SnapshotArgs),
    #[command(about="Manage optional standard hooks.")]
    Hooks(HooksArgs),
    #[command(name="__hook", hide=true)]
    InternalHook(InternalHookArgs),
    #[command(about="Process one JSON API v1 request from stdin.")]
    Api,
}

#[derive(Args)]
struct AddArgs {
    #[arg(help="Task title")]
    title: String,
    #[arg(short='l', long="label", help="Attach a label (repeatable)")]
    labels: Vec<String>,
    #[arg(short='s', long="status", default_value="todo", help="Initial status")]
    status: String,
    #[arg(short='b', long="body", default_value="", help="Initial description body")]
    body: String,
}
#[derive(Args)]
struct ListArgs {
    #[arg(short='n', long="limit", default_value_t=0, allow_negative_numbers=true, help="Show only N newest tasks (0 = all)")]
    limit: i64,
    #[arg(short='s', long="status", help="Filter by status")]
    status: Option<String>,
    #[arg(short='l', long="label", help="Filter by label (repeatable, AND logic)")]
    labels: Vec<String>,
    #[arg(short='q', long="query", help="Search query (case-insensitive, matches title/body/labels/status)")]
    query: Option<String>,
    #[arg(short='r', long="reverse", help="Reverse sort order (oldest first)")]
    reverse: bool,
    #[arg(long="scan", help="Bypass index, scan .md files directly")]
    scan: bool,
    #[arg(long="archived", help="List archived tasks instead of active tasks (SQLite mode)")]
    archived: bool,
}
#[derive(Args)]
struct SeeArgs {
    #[arg(help="Task ID, e.g. #1 or 1")]
    id: String,
}
#[derive(Args)]
struct EditArgs {
    #[arg(help="Task ID to edit")]
    id: String,
}
#[derive(Args)]
struct BodyArgs {
    #[arg(help="Task ID")]
    id: String,
    #[arg(help="New description body text")]
    text: String,
}
#[derive(Args)]
struct AppendArgs {
    #[arg(help="Task ID")]
    id: String,
    #[arg(help="Text to append to the description body")]
    text: String,
}
#[derive(Args)]
struct StatusArgs {
    id: String,
    #[arg(help="New status: todo, in_progress, done, cancelled")]
    new_status: String,
}
#[derive(Args)]
struct LabelArgs {
    id: String,
    #[arg(required=true, num_args=1.., help="Label operations: +label to add, -label to remove")]
    ops: Vec<String>,
    #[arg(long="clear", action=clap::ArgAction::SetTrue, overrides_with="no_clear", help="Clear all labels before applying operations")]
    clear: bool,
    #[arg(long="no-clear", action=clap::ArgAction::SetTrue, overrides_with="clear")]
    no_clear: bool,
}
#[derive(Args)]
struct CommitArgs {
    id: String,
    sha: String,
}
#[derive(Args)]
struct DeleteArgs {
    id: String,
    #[arg(short='f', long="force", help="Skip confirmation")]
    force: bool,
}

#[derive(Args)]
struct TaskIdArgs { id: String }

#[derive(Args)]
struct VersionArgs {
    id: Option<String>,
    version_no: Option<i64>,
}

#[derive(Args)]
struct CancelArgs {
    id: String,
    #[arg(long)]
    reason: Option<String>,
}

#[derive(Args)]
struct ReopenArgs {
    id: String,
    #[arg(long)]
    status: Option<String>,
}

#[derive(Args)]
struct DependsArgs {
    #[command(subcommand)]
    command: DependsCommands,
}

#[derive(Subcommand)]
enum DependsCommands {
    Add { id: String, depends_on: String },
    Remove { id: String, depends_on: String },
    List { id: String },
}

#[derive(Args)]
struct HistoryArgs {
    id: String,
    #[arg(long, default_value_t=100)]
    limit: usize,
}

#[derive(Args)]
struct LockArgs {
    #[command(subcommand)]
    command: LockCommands,
}

#[derive(Subcommand)]
enum LockCommands {
    Acquire { id: String },
    Recover { id: String },
    Heartbeat { id: String, owner_id: String, fencing_generation: i64 },
    Release { id: String, owner_id: String, fencing_generation: i64 },
}

#[derive(Args)]
struct InitArgs {
    #[arg(long)]
    project_id: Option<String>,
}

#[derive(Args)]
struct MigrateArgs {
    #[arg(long)]
    project_id: String,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Args)]
struct SnapshotArgs {
    path: PathBuf,
    #[arg(long)]
    overwrite: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EditableFrontmatter {
    id: i64,
    added_at: String,
    status: String,
    title: String,
    #[serde(default)]
    labels: Vec<String>,
    #[serde(default)]
    commits: Vec<String>,
    #[serde(default)]
    archived_at: Option<String>,
    #[serde(default)]
    archived_by: Option<String>,
}

#[derive(Args)]
struct HooksArgs {
    #[command(subcommand)]
    command: HooksCommands,
}

#[derive(Subcommand)]
enum HooksCommands {
    Status,
    Install {
        #[arg(value_enum)]
        hook: HookSelection,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum HookSelection {
    Git,
    Openspec,
    All,
}

#[derive(Args)]
struct InternalHookArgs {
    name: String,
}

fn fail(message: impl AsRef<str>) -> ! {
    eprintln!("Error: {}", message.as_ref());
    std::process::exit(1)
}
fn task_or_fail(store: &Store, raw: &str) -> (i64, LegacyTask) {
    let id = Store::resolve_id(raw).unwrap_or_else(|e| fail(e));
    let task = store
        .read_task(id)
        .unwrap_or_else(|e| fail(e))
        .unwrap_or_else(|| fail(format!("Task #{} not found.", id)));
    (id, task)
}
fn emit_json<T: Serialize>(value: &T) {
    println!("{}", serde_json::to_string(value).unwrap_or_else(|e| fail(e.to_string())));
}
fn emit_task(task: &LegacyTask, json: bool, human: String) {
    if json { emit_json(task); } else { println!("{}", human); }
}

fn run_hook(name: &str, task: &LegacyTask) -> Result<(), String> {
    let hooks = PathBuf::from(".todoing").join("hooks");
    #[cfg(windows)]
    let candidates = [hooks.join(format!("{}.cmd", name)), hooks.join(format!("{}.bat", name))];
    #[cfg(not(windows))]
    let candidates = [hooks.join(name)];
    let Some(path) = candidates.iter().find(|p| p.is_file()) else { return Ok(()); };

    #[cfg(windows)]
    let mut command = Command::new("cmd");
    #[cfg(windows)]
    command.arg("/C").arg(path);
    #[cfg(not(windows))]
    let mut command = Command::new(path);

    let mut child = command.stdin(Stdio::piped()).stdout(Stdio::inherit()).stderr(Stdio::inherit())
        .spawn().map_err(|e| format!("Cannot start {} hook: {}", name, e))?;
    let payload = serde_json::to_vec(task).map_err(|e| e.to_string())?;
    let mut stdin = child.stdin.take().ok_or_else(|| "Hook stdin unavailable".to_string())?;
    let write_result = stdin.write_all(&payload);
    drop(stdin);
    if let Err(error) = write_result {
        if error.kind() != std::io::ErrorKind::BrokenPipe {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error.to_string());
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(format!("{} hook rejected the operation (exit code {}).", name, status.code().unwrap_or(1)));
    }
    Ok(())
}


fn standard_hook_names(selection: HookSelection) -> Vec<&'static str> {
    match selection {
        HookSelection::Git => vec!["before-done-git"],
        HookSelection::Openspec => vec!["before-done-openspec"],
        HookSelection::All => vec!["before-done-git", "before-done-openspec"],
    }
}

#[cfg(windows)]
fn hook_candidates(name: &str) -> Vec<PathBuf> {
    let dir = PathBuf::from(".todoing").join("hooks");
    vec![dir.join(format!("{}.cmd", name)), dir.join(format!("{}.bat", name))]
}
#[cfg(not(windows))]
fn hook_candidates(name: &str) -> Vec<PathBuf> {
    vec![PathBuf::from(".todoing").join("hooks").join(name)]
}

fn hook_installed(name: &str) -> bool {
    hook_candidates(name).iter().any(|path| path.is_file())
}

fn bundled_hook(name: &str) -> Result<&'static str, String> {
    #[cfg(windows)]
    {
        match name {
            "before-done-git" => Ok(GIT_HOOK_WINDOWS),
            "before-done-openspec" => Ok(OPENSPEC_HOOK_WINDOWS),
            _ => Err(format!("Unknown standard hook '{}'.", name)),
        }
    }
    #[cfg(not(windows))]
    {
        match name {
            "before-done-git" => Ok(GIT_HOOK_UNIX),
            "before-done-openspec" => Ok(OPENSPEC_HOOK_UNIX),
            _ => Err(format!("Unknown standard hook '{}'.", name)),
        }
    }
}

fn install_hook(name: &str) -> Result<bool, String> {
    if hook_installed(name) { return Ok(false); }
    let dir = PathBuf::from(".todoing").join("hooks");
    fs::create_dir_all(&dir).map_err(|e| format!("Cannot create hooks directory: {}", e))?;
    let target = hook_candidates(name).into_iter().next().ok_or_else(|| "No hook path available".to_string())?;
    let mut temp = NamedTempFile::new_in(&dir).map_err(|e| format!("Cannot create temporary hook file: {}", e))?;
    temp.write_all(bundled_hook(name)?.as_bytes()).map_err(|e| format!("Cannot write hook: {}", e))?;
    temp.as_file().sync_all().map_err(|e| format!("Cannot sync hook: {}", e))?;
    temp.persist(&target).map_err(|e| format!("Cannot install hook: {}", e.error))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&target).map_err(|e| e.to_string())?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&target, permissions).map_err(|e| format!("Cannot make hook executable: {}", e))?;
    }
    Ok(true)
}

fn show_hook_status(json: bool) {
    let hooks = ["before-done-git", "before-done-openspec"];
    let missing = hooks.iter().any(|name| !hook_installed(name));
    if json {
        let rows: Vec<_> = hooks.iter().map(|name| serde_json::json!({
            "name": name,
            "installed": hook_installed(name)
        })).collect();
        cli_success(true, serde_json::json!({
            "hooks": rows,
            "agent_action": if missing { "ask_user_before_install" } else { "none" }
        }), "");
    } else {
        for name in hooks {
            println!("{}: {}", name, if hook_installed(name) { "installed" } else { "not installed" });
        }
        if missing {
            println!("Some standard hooks are missing. Ask the user before installing them.");
        }
    }
}

fn install_hooks(selection: HookSelection, json: bool) {
    let mut installed = Vec::new();
    let mut already_installed = Vec::new();
    for name in standard_hook_names(selection) {
        match install_hook(name) {
            Ok(true) => installed.push(name),
            Ok(false) => already_installed.push(name),
            Err(error) => fail(error),
        }
    }
    if json {
        cli_success(true, serde_json::json!({
            "installed": installed,
            "already_installed": already_installed
        }), "");
    } else {
        for name in installed { println!("Installed {}.", name); }
        for name in already_installed { println!("{} is already present; left unchanged.", name); }
    }
}

fn run_internal_hook(name: &str) -> Result<(), String> {
    let mut payload = Vec::new();
    std::io::stdin().read_to_end(&mut payload).map_err(|e| format!("Cannot read task data: {}", e))?;
    let task: LegacyTask = serde_json::from_slice(&payload).map_err(|e| format!("Invalid task data: {}", e))?;
    match name {
        "before-done-git" => {
            if task.commits.is_empty() {
                return Err(format!("Task #{} has no linked Git commits.", task.id));
            }
            for sha in &task.commits {
                let spec = format!("{}^{{commit}}", sha);
                let output = Command::new("git").arg("cat-file").arg("-e").arg(&spec).output()
                    .map_err(|e| format!("Cannot run git: {}", e))?;
                if !output.status.success() {
                    return Err(format!("Linked Git commit {} for task #{} does not exist in this repository.", sha, task.id));
                }
            }
            Ok(())
        }
        "before-done-openspec" => {
            let changes = PathBuf::from("openspec/changes");
            if !changes.is_dir() { return Ok(()); }
            let entries = fs::read_dir(&changes).map_err(|e| format!("Cannot read OpenSpec changes directory: {}", e))?;
            for entry in entries {
                let entry = entry.map_err(|e| e.to_string())?;
                let path = entry.path();
                if !path.is_dir() || entry.file_name().to_string_lossy() == "archive" { continue; }
                let tasks_file = path.join("tasks.md");
                if !tasks_file.is_file() { continue; }
                let content = fs::read_to_string(&tasks_file)
                    .map_err(|e| format!("Cannot read {}: {}", tasks_file.display(), e))?;
                for (index, line) in content.lines().enumerate() {
                    let item = line.trim_start();
                    if item.starts_with("- [ ]") || item.starts_with("* [ ]") || item.starts_with("+ [ ]") {
                        return Err(format!("OpenSpec checklist is incomplete: {}:{}: {}", tasks_file.display(), index + 1, item));
                    }
                }
            }
            Ok(())
        }
        _ => Err(format!("Unknown internal hook '{}'.", name)),
    }
}

fn run_legacy(cli: Cli) {
    let json = cli.json;
    let store = Store::current();
    match cli.command {
        Some(Commands::Add(a)) => {
            let status = TaskStatus::parse(&a.status).unwrap_or_else(|e| fail(e));
            let task = LegacyTask::new(store.next_id(), a.title, status, a.labels, a.body);
            task.validate_for_creation().unwrap_or_else(|e| fail(e));
            if task.status == TaskStatus::Done {
                run_hook("before-done-git", &task).unwrap_or_else(|e| fail(e));
                run_hook("before-done-openspec", &task).unwrap_or_else(|e| fail(e));
                run_hook("before-done", &task).unwrap_or_else(|e| fail(e));
            }
            run_hook("on-create", &task).unwrap_or_else(|e| fail(e));
            store.write_task(&task).unwrap_or_else(|e| fail(e));
            store.sync_mutation(&task).unwrap_or_else(|e| fail(e));
            emit_task(&task, json, format!("(#{}) {}", task.id, task.title));
        }
        Some(Commands::Ls(a)) => {
            if a.archived {
                cli_failure(json, "invalid_request", 2, "--archived requires the SQLite store; run 'todoing migrate' first", false);
            }
            if a.scan { store.rebuild_index().unwrap_or_else(|e| fail(e)); }
            let status = a.status.as_deref().map(TaskStatus::parse).transpose().unwrap_or_else(|e| fail(e));
            let tasks = store.filter(status, &a.labels, a.limit, a.query.as_deref(), a.reverse).unwrap_or_else(|e| fail(e));
            if json { emit_json(&tasks); }
            else if tasks.is_empty() { println!("No tasks."); }
            else { for t in tasks { println!("{}", t.format_list()); } }
        }
        Some(Commands::See(a)) => {
            let (_, task) = task_or_fail(&store, &a.id);
            if json { emit_json(&task); } else { print!("{}", task.format_see()); }
        }
        Some(Commands::Edit(a)) => {
            let id = Store::resolve_id(&a.id).unwrap_or_else(|e| fail(e));
            let path = std::env::current_dir().unwrap_or_default().join(".todoing/tasks").join(format!("{}.md", id));
            if !path.is_file() { fail(format!("Task #{} not found.", id)); }
            let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".into());
            let status = Command::new(editor).arg(path).status().unwrap_or_else(|e| fail(e.to_string()));
            if !status.success() { fail(format!("Editor exited with code {}", status.code().unwrap_or(1))); }
            store.rebuild_index().unwrap_or_else(|e| fail(e));
            if json { let (_, task) = task_or_fail(&store, &id.to_string()); emit_json(&task); }
        }
        Some(Commands::Body(a)) => {
            let (_, mut task) = task_or_fail(&store, &a.id);
            task.body = a.text;
            store.write_task(&task).unwrap_or_else(|e| fail(e));
            store.sync_mutation(&task).unwrap_or_else(|e| fail(e));
            emit_task(&task, json, format!("(#{}) body updated.", task.id));
        }
        Some(Commands::Append(a)) => {
            let (_, mut task) = task_or_fail(&store, &a.id);
            if task.body.is_empty() { task.body = a.text; } else { task.body = format!("{}\n\n{}", task.body, a.text); }
            store.write_task(&task).unwrap_or_else(|e| fail(e));
            store.sync_mutation(&task).unwrap_or_else(|e| fail(e));
            emit_task(&task, json, format!("(#{}) body appended.", task.id));
        }
        Some(Commands::Status(a)) => {
            let (_, mut task) = task_or_fail(&store, &a.id);
            let next = TaskStatus::parse(&a.new_status).unwrap_or_else(|e| fail(e));
            if next == TaskStatus::Done && task.status != TaskStatus::Done {
                run_hook("before-done-git", &task).unwrap_or_else(|e| fail(e));
                run_hook("before-done-openspec", &task).unwrap_or_else(|e| fail(e));
                run_hook("before-done", &task).unwrap_or_else(|e| fail(e));
            }
            task.status = next;
            store.write_task(&task).unwrap_or_else(|e| fail(e));
            store.sync_mutation(&task).unwrap_or_else(|e| fail(e));
            emit_task(&task, json, format!("(#{}) status -> {{{}}} {}", task.id, task.status.icon(), task.status.as_str()));
        }
        Some(Commands::Label(a)) => {
            let (_, mut task) = task_or_fail(&store, &a.id);
            let mut current = if a.clear && !a.no_clear { Vec::new() } else { task.labels.clone() };
            let mut adds = Vec::new();
            let mut removes = std::collections::HashSet::new();
            for op in a.ops {
                if let Some(v) = op.strip_prefix('+') { adds.push(v.to_string()); }
                else if let Some(v) = op.strip_prefix('-') { removes.insert(v.to_string()); }
                else { fail(format!("Invalid label operation '{}'. Use +label to add or -label to remove.", op)); }
            }
            for r in removes { if let Some(pos) = current.iter().position(|x| x == &r) { current.remove(pos); } }
            current.extend(adds);
            let mut seen = std::collections::HashSet::new();
            task.labels = current.into_iter().filter(|x| seen.insert(x.clone())).collect();
            store.write_task(&task).unwrap_or_else(|e| fail(e));
            store.sync_mutation(&task).unwrap_or_else(|e| fail(e));
            emit_task(&task, json, format!("(#{}) labels -> [{}]", task.id, task.labels.join(", ")));
        }
        Some(Commands::Commit(a)) => {
            let (_, mut task) = task_or_fail(&store, &a.id);
            let spec = format!("{}^{{commit}}", a.sha);
            let output = Command::new("git").arg("rev-parse").arg("--verify").arg(&spec).output()
                .unwrap_or_else(|e| fail(format!("Cannot run git: {}", e)));
            if !output.status.success() { fail(format!("'{}' is not a valid Git commit in this repository.", a.sha)); }
            let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !task.commits.contains(&sha) { task.commits.push(sha); }
            store.write_task(&task).unwrap_or_else(|e| fail(e));
            store.sync_mutation(&task).unwrap_or_else(|e| fail(e));
            emit_task(&task, json, format!("(#{}) commit linked.", task.id));
        }
        Some(Commands::Delete(_)) => {
            cli_failure(json, "invalid_request", 2, "Physical deletion is disabled. Use 'todoing cancel <id>' to retain the task and record its cancellation.", false);
        }
        Some(Commands::Reindex) => {
            let tasks = store.rebuild_index().unwrap_or_else(|e| fail(e));
            if json { emit_json(&tasks); } else { println!("Index rebuilt: {} tasks.", tasks.len()); }
        }
        Some(Commands::Version(args)) if args.id.is_none() => {
            if json { emit_json(&serde_json::json!({"version": VERSION})); }
            else { println!("{}", VERSION); }
        }
        Some(Commands::Hooks(args)) => match args.command {
            HooksCommands::Status => show_hook_status(json),
            HooksCommands::Install { hook } => install_hooks(hook, json),
        },
        Some(Commands::InternalHook(args)) => run_internal_hook(&args.name).unwrap_or_else(|e| fail(e)),
        Some(Commands::Api) => todoing::api::run(),
        Some(Commands::Init(_)) => fail("The init command requires the SQLite initialization path."),
        Some(Commands::Archive(_)) | Some(Commands::Restore(_)) => {
            cli_failure(json, "invalid_request", 2, "Archive and restore require the SQLite store. Run 'todoing migrate --project-id <id>' first.", false);
        },
        Some(Commands::Version(_)) |
        Some(Commands::Complete(_)) |
        Some(Commands::Cancel(_)) |
        Some(Commands::Reopen(_)) |
        Some(Commands::Depends(_)) |
        Some(Commands::History(_)) |
        Some(Commands::Lock(_)) |
        Some(Commands::Config) |
        Some(Commands::Migrate(_)) |
        Some(Commands::Verify) |
        Some(Commands::Snapshot(_)) => fail("This command requires the SQLite store. Run 'todoing migrate --project-id <id>' first."),
        None => {}
    }
}

struct SqliteContext {
    store: SqliteStore,
    config: ProjectConfig,
    repository: TaskRepository,
    lock_service: todoing::storage::TaskLockService,
    actor: ActorContext,
}

fn cli_success(json_output: bool, data: serde_json::Value, human: impl AsRef<str>) {
    if json_output {
        println!("{}", serde_json::json!({
            "api_version": 1,
            "ok": true,
            "data": data
        }));
    } else {
        println!("{}", human.as_ref());
    }
}

fn cli_failure(json_output: bool, code: &str, exit_code: i32, message: impl AsRef<str>, retryable: bool) -> ! {
    let message = message.as_ref();
    eprintln!("Error: {message}");
    if json_output {
        println!("{}", serde_json::json!({
            "api_version": 1,
            "ok": false,
            "error": {
                "code": code,
                "message": message,
                "retryable": retryable,
                "details": {}
            }
        }));
    }
    std::process::exit(exit_code);
}

fn fail_store(json_output: bool, error: StoreError) -> ! {
    let (code, exit_code, retryable) = match &error {
        StoreError::TaskNotFound(_) => ("task_not_found", 3, false),
        StoreError::VersionNotFound { .. } => ("version_not_found", 3, false),
        StoreError::DependencyNotFound(_) => ("dependency_not_found", 3, false),
        StoreError::TaskTerminal => ("task_terminal", 4, false),
        StoreError::TaskArchived => ("task_archived", 4, false),
        StoreError::TaskLocked => ("task_locked", 4, true),
        StoreError::LockExpired => ("lock_expired", 4, false),
        StoreError::StaleLock => ("stale_lock", 4, false),
        StoreError::InvalidStatusTransition(_) => ("invalid_status_transition", 4, false),
        StoreError::DependencyCycle => ("dependency_cycle", 4, false),
        StoreError::InvalidConfig(_) => ("config_invalid", 2, false),
        StoreError::ProjectIdMismatch { .. } => ("project_id_mismatch", 2, false),
        StoreError::AlreadyExists(_) => ("invalid_request", 2, false),
        StoreError::NotInitialized => ("migration_conflict", 6, false),
        StoreError::InvalidTask(_) => ("invalid_request", 2, false),
        StoreError::HistoryRecordFailed(_) => ("history_record_failed", 5, false),
        StoreError::IntegrityCheckFailed(_) | StoreError::ForeignKeyViolations(_) |
        StoreError::MigrationChecksumMismatch { .. } | StoreError::UnsupportedSchemaVersion(_) |
        StoreError::MissingMetadata(_) | StoreError::InvalidMetadata(_) => ("store_corrupt", 5, false),
        StoreError::Sqlite(rusqlite::Error::SqliteFailure(code, _))
            if code.code == rusqlite::ErrorCode::DatabaseBusy ||
               code.code == rusqlite::ErrorCode::DatabaseLocked => ("store_busy", 5, true),
        StoreError::Sqlite(_) | StoreError::Io(_) => ("io_error", 5, false),
    };
    cli_failure(json_output, code, exit_code, error.to_string(), retryable)
}

fn parse_cli_task_id(raw: &str, json_output: bool) -> TaskId {
    let parsed = raw.strip_prefix('#').unwrap_or(raw).parse::<i64>()
        .ok().and_then(|value| TaskId::new(value).ok());
    parsed.unwrap_or_else(|| cli_failure(json_output, "invalid_request", 2, format!("Invalid task ID '{raw}'"), false))
}

fn open_sqlite_context(root: &std::path::Path, actor_raw: &str, json_output: bool) -> SqliteContext {
    let config_path = root.join(".todoing").join("config.toml");
    let config_text = fs::read_to_string(&config_path).unwrap_or_else(|error| {
        cli_failure(json_output, "config_invalid", 2, format!("Cannot read {}: {error}", config_path.display()), false)
    });
    let config = ProjectConfig::from_toml(&config_text).unwrap_or_else(|error| {
        cli_failure(json_output, "config_invalid", 2, error.to_string(), false)
    });
    let actor_id = ActorId::new(actor_raw.to_owned()).unwrap_or_else(|error| {
        cli_failure(json_output, "invalid_request", 2, error.to_string(), false)
    });
    let store = SqliteStore::open(root, None).unwrap_or_else(|error| fail_store(json_output, error));
    let repository = TaskRepository::new(store.clone(), config.clone())
        .unwrap_or_else(|error| fail_store(json_output, error));
    let lock_service = repository.lock_service().unwrap_or_else(|error| fail_store(json_output, error));
    SqliteContext { store, config, repository, lock_service, actor: ActorContext::new(actor_id) }
}

fn repository_for_task(context: &SqliteContext, task_id: TaskId, json_output: bool) -> TaskRepository {
    match std::env::var("TODOING_LOCK_TOKEN") {
        Ok(token) => {
            let grant = context.lock_service.grant_from_token(task_id, &token)
                .unwrap_or_else(|error| fail_store(json_output, error));
            context.repository.with_lock_grant(&grant)
        }
        Err(_) => context.repository.clone(),
    }
}

fn status_icon(task: &SqliteTask) -> &'static str {
    match task.outcome {
        Some(TaskOutcome::Completed) => "✓",
        Some(TaskOutcome::Cancelled) => "×",
        None if task.status_id.as_str() == "in_progress" => "▶",
        None if task.status_id.as_str() == "todo" => "·",
        None => "•",
    }
}

fn format_sqlite_list(task: &SqliteTask) -> String {
    let archived = if task.archived_at.is_some() { "[archived] " } else { "" };
    format!("{}(#{}) [{}] {{{}}} {}", archived, task.id, task.labels.join(","), status_icon(task), task.title)
}

fn format_sqlite_see(task: &SqliteTask) -> String {
    let frontmatter = serde_json::json!({
        "added_at": task.created_at,
        "archived_at": task.archived_at,
        "archived_by": task.archived_by.as_ref().map(ActorId::as_str),
        "commits": task.commit_refs,
        "id": task.id.get(),
        "labels": task.labels,
        "status": task.status_id.as_str(),
        "title": task.title
    });
    let yaml = serde_yaml::to_string(&frontmatter).unwrap_or_else(|error| format!("title: {:?}\n", error.to_string()));
    if task.body.trim().is_empty() {
        format!("---\n{}---\n", yaml)
    } else {
        format!("---\n{}---\n\n{}\n", yaml, task.body.trim())
    }
}

fn parse_edited_task(source: &str, original: &SqliteTask, json_output: bool) -> SqliteTask {
    let rest = source.strip_prefix("---\n")
        .unwrap_or_else(|| cli_failure(json_output, "invalid_request", 2, "Edited task must retain YAML frontmatter", false));
    let (frontmatter, body) = rest.split_once("\n---")
        .unwrap_or_else(|| cli_failure(json_output, "invalid_request", 2, "Edited task has no closing frontmatter delimiter", false));
    let parsed: EditableFrontmatter = serde_yaml::from_str(frontmatter)
        .unwrap_or_else(|error| cli_failure(json_output, "invalid_request", 2, format!("Invalid task frontmatter: {error}"), false));
    if parsed.id != original.id.get() || parsed.added_at != original.created_at {
        cli_failure(json_output, "invalid_request", 2, "Task ID and creation timestamp are immutable", false);
    }
    let status_id = StatusId::new(parsed.status)
        .unwrap_or_else(|error| cli_failure(json_output, "invalid_request", 2, error.to_string(), false));
    let body = body.strip_prefix('\n').unwrap_or(body).trim().to_owned();
    SqliteTask {
        id: original.id,
        title: parsed.title,
        status_id,
        outcome: original.outcome,
        body,
        created_at: original.created_at.clone(),
        updated_at: original.updated_at.clone(),
        archived_at: original.archived_at.clone(),
        archived_by: original.archived_by.clone(),
        labels: parsed.labels,
        commit_refs: parsed.commits,
        dependencies: original.dependencies.clone(),
    }
}

fn new_lock_owner_id(json_output: bool) -> String {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).unwrap_or_else(|error| {
        cli_failure(json_output, "internal_error", 70, format!("Cannot generate lock owner ID: {error}"), false)
    });
    format!("session-{}", hex::encode(bytes))
}

fn lock_grant_data(grant: &LockGrant) -> serde_json::Value {
    serde_json::json!({
        "owner_id": grant.owner_id,
        "lock_token": grant.token,
        "fencing_generation": grant.fencing_generation,
        "acquired_at": grant.acquired_at,
        "heartbeat_at": grant.heartbeat_at,
        "expires_at": grant.expires_at
    })
}

fn resolve_git_commit(root: &std::path::Path, revision: &str, json_output: bool) -> String {
    let output = Command::new("git")
        .current_dir(root)
        .args(["rev-parse", "--verify", "--end-of-options"])
        .arg(format!("{revision}^{{commit}}"))
        .output()
        .unwrap_or_else(|error| cli_failure(json_output, "io_error", 5, format!("Cannot run git: {error}"), false));
    if !output.status.success() {
        cli_failure(json_output, "invalid_request", 2, format!("'{revision}' is not a valid Git commit in this repository"), false);
    }
    let sha = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if !((sha.len() == 40 || sha.len() == 64) && sha.bytes().all(|byte| byte.is_ascii_hexdigit())) {
        cli_failure(json_output, "invalid_request", 2, "Git returned an invalid commit object ID", false);
    }
    sha
}

fn run_sqlite(cli: Cli) {
    let json_output = cli.json;
    let root = std::env::current_dir().unwrap_or_else(|error| cli_failure(json_output, "io_error", 5, error.to_string(), false));
    let actor_raw = cli.actor
        .or_else(|| std::env::var("TODOING_ACTOR_ID").ok())
        .unwrap_or_else(|| "cli-user".into());

    if let Some(Commands::Init(args)) = &cli.command {
        let project_id = args.project_id.clone().map(ProjectId::new).transpose().unwrap_or_else(|error| {
            cli_failure(json_output, "invalid_request", 2, error.to_string(), false)
        });
        let report = todoing::initialization::initialize(&root, project_id).unwrap_or_else(|error| {
            let (code, exit) = match &error {
                StoreError::AlreadyExists(_) => ("store_already_initialized", 6),
                StoreError::InvalidConfig(_) => ("config_invalid", 2),
                StoreError::InvalidTask(_) => ("migration_required", 2),
                StoreError::Io(_) => ("io_error", 5),
                _ => ("internal_error", 70),
            };
            cli_failure(json_output, code, exit, error.to_string(), false)
        });
        cli_success(json_output, serde_json::json!({
            "project_id": report.project_id,
            "schema_version": report.schema_version,
            "api_version": 1
        }), format!("Initialized Todoing project {} (schema {})", report.project_id, report.schema_version));
        return;
    }
    if matches!(&cli.command, Some(Commands::Api)) {
        todoing::api::run();
        return;
    }
    if let Some(Commands::Version(args)) = &cli.command {
        if args.id.is_none() {
            cli_success(json_output, serde_json::json!({"version": VERSION}), VERSION);
            return;
        }
    }
    if let Some(Commands::Migrate(args)) = &cli.command {
        let project_id = ProjectId::new(args.project_id.clone()).unwrap_or_else(|error| {
            cli_failure(json_output, "invalid_request", 2, error.to_string(), false)
        });
        let actor_id = ActorId::new(actor_raw).unwrap_or_else(|error| {
            cli_failure(json_output, "invalid_request", 2, error.to_string(), false)
        });
        let report = migration::migrate(&root, &project_id, &actor_id, args.dry_run)
            .unwrap_or_else(|error| {
                let (code, exit) = match &error {
                    StoreError::AlreadyExists(_) | StoreError::InvalidTask(_) => ("migration_conflict", 6),
                    StoreError::InvalidConfig(_) => ("config_invalid", 2),
                    StoreError::Io(_) => ("io_error", 5),
                    _ => ("internal_error", 70),
                };
                cli_failure(json_output, code, exit, error.to_string(), false)
            });
        let data = serde_json::to_value(&report).unwrap_or_else(|error| {
            cli_failure(json_output, "internal_error", 70, error.to_string(), false)
        });
        cli_success(json_output, data, format!("Migration validated: {} tasks; activated={}", report.task_count, report.activated));
        return;
    }

    let context = open_sqlite_context(&root, &actor_raw, json_output);
    match cli.command {
        Some(Commands::Add(args)) => {
            let status_id = StatusId::new(args.status).unwrap_or_else(|error| {
                cli_failure(json_output, "invalid_request", 2, error.to_string(), false)
            });
            let task = context.repository.create(
                NewTask { title: args.title, status_id, outcome: None, body: args.body, labels: args.labels, commit_refs: vec![] },
                &context.actor,
            ).unwrap_or_else(|error| fail_store(json_output, error));
            let data = serde_json::to_value(&task).unwrap();
            cli_success(json_output, data, format!("(#{}) {}", task.id, task.title));
        }
        Some(Commands::Ls(args)) => {
            if args.scan {
                cli_failure(json_output, "invalid_request", 2, "--scan is deprecated for SQLite; use 'todoing verify'", false);
            }
            let mut tasks = if args.archived {
                context.repository.list_archived()
            } else {
                context.repository.list()
            }.unwrap_or_else(|error| fail_store(json_output, error));
            if let Some(status) = args.status {
                tasks.retain(|task| task.status_id.as_str().eq_ignore_ascii_case(&status));
            }
            for label in args.labels {
                tasks.retain(|task| task.labels.iter().any(|value| value.eq_ignore_ascii_case(&label)));
            }
            if let Some(query) = args.query {
                let query = query.to_lowercase();
                tasks.retain(|task| task.title.to_lowercase().contains(&query)
                    || task.body.to_lowercase().contains(&query)
                    || task.labels.iter().any(|label| label.to_lowercase().contains(&query))
                    || task.status_id.as_str().to_lowercase().contains(&query));
            }
            tasks.sort_by_key(|task| task.id);
            if !args.reverse { tasks.reverse(); }
            if args.limit > 0 { tasks.truncate(args.limit as usize); }
            let data = serde_json::to_value(&tasks).unwrap();
            let human = if tasks.is_empty() { "No tasks.".into() }
                else { tasks.iter().map(format_sqlite_list).collect::<Vec<_>>().join("\n") };
            cli_success(json_output, data, human);
        }
        Some(Commands::See(args)) => {
            let task_id = parse_cli_task_id(&args.id, json_output);
            let repository = repository_for_task(&context, task_id, json_output);
            let task = repository.get(task_id).unwrap_or_else(|error| fail_store(json_output, error));
            let data = serde_json::to_value(&task).unwrap();
            cli_success(json_output, data, format_sqlite_see(&task));
        }
        Some(Commands::Edit(args)) => {
            let task_id = parse_cli_task_id(&args.id, json_output);
            let repository = repository_for_task(&context, task_id, json_output);
            let original = repository.get(task_id).unwrap_or_else(|error| fail_store(json_output, error));
            let mut temporary = NamedTempFile::new_in(root.join(".todoing"))
                .unwrap_or_else(|error| cli_failure(json_output, "io_error", 5, error.to_string(), false));
            temporary.write_all(format_sqlite_see(&original).as_bytes())
                .unwrap_or_else(|error| cli_failure(json_output, "io_error", 5, error.to_string(), false));
            let path = temporary.into_temp_path();
            let editor = std::env::var("EDITOR").unwrap_or_else(|_| if cfg!(windows) { "notepad".into() } else { "vim".into() });
            let status = Command::new(editor).arg(&path).status()
                .unwrap_or_else(|error| cli_failure(json_output, "io_error", 5, error.to_string(), false));
            if !status.success() {
                cli_failure(json_output, "io_error", 5, format!("Editor exited with code {}", status.code().unwrap_or(1)), false);
            }
            let edited = fs::read_to_string(&path)
                .unwrap_or_else(|error| cli_failure(json_output, "io_error", 5, error.to_string(), false));
            let updated = parse_edited_task(&edited, &original, json_output);
            let task = repository.update(&updated, &context.actor)
                .unwrap_or_else(|error| fail_store(json_output, error));
            cli_success(json_output, serde_json::to_value(&task).unwrap(), format!("(#{}) updated.", task.id));
        }
        Some(Commands::Body(args)) => {
            let task_id = parse_cli_task_id(&args.id, json_output);
            let repository = repository_for_task(&context, task_id, json_output);
            let mut task = repository.get(task_id).unwrap_or_else(|error| fail_store(json_output, error));
            task.body = args.text;
            let updated = repository.update(&task, &context.actor).unwrap_or_else(|error| fail_store(json_output, error));
            cli_success(json_output, serde_json::to_value(&updated).unwrap(), format!("(#{}) body updated.", updated.id));
        }
        Some(Commands::Append(args)) => {
            let task_id = parse_cli_task_id(&args.id, json_output);
            let repository = repository_for_task(&context, task_id, json_output);
            let mut task = repository.get(task_id).unwrap_or_else(|error| fail_store(json_output, error));
            task.body = if task.body.is_empty() { args.text } else { format!("{}\n\n{}", task.body, args.text) };
            let updated = repository.update(&task, &context.actor).unwrap_or_else(|error| fail_store(json_output, error));
            cli_success(json_output, serde_json::to_value(&updated).unwrap(), format!("(#{}) body appended.", updated.id));
        }
        Some(Commands::Status(args)) => {
            let task_id = parse_cli_task_id(&args.id, json_output);
            let repository = repository_for_task(&context, task_id, json_output);
            let status_id = StatusId::new(args.new_status).unwrap_or_else(|error| {
                cli_failure(json_output, "invalid_request", 2, error.to_string(), false)
            });
            let updated = repository.set_status(task_id, status_id, &context.actor)
                .unwrap_or_else(|error| fail_store(json_output, error));
            cli_success(json_output, serde_json::to_value(&updated).unwrap(), format!("(#{}) status -> {}", updated.id, updated.status_id));
        }
        Some(Commands::Label(args)) => {
            let task_id = parse_cli_task_id(&args.id, json_output);
            let repository = repository_for_task(&context, task_id, json_output);
            let mut task = repository.get(task_id).unwrap_or_else(|error| fail_store(json_output, error));
            if args.clear && !args.no_clear { task.labels.clear(); }
            for operation in args.ops {
                if let Some(label) = operation.strip_prefix('+') {
                    if !task.labels.iter().any(|existing| existing.eq_ignore_ascii_case(label)) {
                        task.labels.push(label.to_owned());
                    }
                } else if let Some(label) = operation.strip_prefix('-') {
                    task.labels.retain(|existing| !existing.eq_ignore_ascii_case(label));
                } else {
                    cli_failure(json_output, "invalid_request", 2, format!("Invalid label operation '{operation}'"), false);
                }
            }
            let updated = repository.update(&task, &context.actor).unwrap_or_else(|error| fail_store(json_output, error));
            cli_success(json_output, serde_json::to_value(&updated).unwrap(), format!("(#{}) labels -> [{}]", updated.id, updated.labels.join(", ")));
        }
        Some(Commands::Commit(args)) => {
            let task_id = parse_cli_task_id(&args.id, json_output);
            let repository = repository_for_task(&context, task_id, json_output);
            let sha = resolve_git_commit(&root, &args.sha, json_output);
            let mut task = repository.get(task_id).unwrap_or_else(|error| fail_store(json_output, error));
            if !task.commit_refs.contains(&sha) { task.commit_refs.push(sha.clone()); }
            let updated = repository.update(&task, &context.actor).unwrap_or_else(|error| fail_store(json_output, error));
            cli_success(json_output, serde_json::to_value(&updated).unwrap(), format!("(#{}) commit {} linked.", updated.id, sha));
        }
        Some(Commands::Delete(_)) => {
            cli_failure(json_output, "invalid_request", 2, "Physical deletion is disabled. Use 'todoing cancel <id>' instead.", false);
        }
        Some(Commands::Reindex) => {
            let report = context.store.verify().unwrap_or_else(|error| fail_store(json_output, error));
            let data = serde_json::json!({"reindexed": false, "reason": "SQLite is authoritative", "integrity": report});
            cli_success(json_output, data, "SQLite store verified; there is no Markdown index to rebuild.");
        }
        Some(Commands::Version(args)) => {
            let Some(raw_id) = args.id else {
                cli_success(json_output, serde_json::json!({"version": VERSION}), VERSION);
                return;
            };
            let task_id = parse_cli_task_id(&raw_id, json_output);
            let repository = repository_for_task(&context, task_id, json_output);
            if let Some(version_no) = args.version_no {
                let version = repository.get_version(task_id, version_no).unwrap_or_else(|error| fail_store(json_output, error));
                cli_success(json_output, serde_json::to_value(&version).unwrap(), format!("Task #{} version {}", task_id, version_no));
            } else {
                let versions = repository.list_versions(task_id).unwrap_or_else(|error| fail_store(json_output, error));
                cli_success(json_output, serde_json::to_value(&versions).unwrap(), format!("Task #{}: {} versions", task_id, versions.len()));
            }
        }
        Some(Commands::Archive(args)) => {
            let task_id = parse_cli_task_id(&args.id, json_output);
            let repository = repository_for_task(&context, task_id, json_output);
            let result = repository.archive(task_id, &context.actor)
                .unwrap_or_else(|error| fail_store(json_output, error));
            cli_success(
                json_output,
                serde_json::json!({"task": result.task, "no_op": result.no_op}),
                format!("Task #{} archived.", task_id),
            );
        }
        Some(Commands::Restore(args)) => {
            let task_id = parse_cli_task_id(&args.id, json_output);
            let repository = repository_for_task(&context, task_id, json_output);
            let result = repository.restore(task_id, &context.actor)
                .unwrap_or_else(|error| fail_store(json_output, error));
            cli_success(
                json_output,
                serde_json::json!({"task": result.task, "no_op": result.no_op}),
                format!("Task #{} restored.", task_id),
            );
        }
        Some(Commands::Complete(args)) => {
            let task_id = parse_cli_task_id(&args.id, json_output);
            let repository = repository_for_task(&context, task_id, json_output);
            let task = repository.complete(task_id, &context.actor).unwrap_or_else(|error| fail_store(json_output, error));
            cli_success(json_output, serde_json::to_value(&task).unwrap(), format!("(#{}) completed.", task.id));
        }
        Some(Commands::Cancel(args)) => {
            let task_id = parse_cli_task_id(&args.id, json_output);
            let repository = repository_for_task(&context, task_id, json_output);
            let reason = args.reason.filter(|value| !value.is_empty());
            let task = repository.cancel(task_id, reason.as_deref(), &context.actor).unwrap_or_else(|error| fail_store(json_output, error));
            cli_success(json_output, serde_json::to_value(&task).unwrap(), format!("(#{}) cancelled; task retained.", task.id));
        }
        Some(Commands::Reopen(args)) => {
            let task_id = parse_cli_task_id(&args.id, json_output);
            let repository = repository_for_task(&context, task_id, json_output);
            let status = args.status.map(StatusId::new).transpose().unwrap_or_else(|error| {
                cli_failure(json_output, "invalid_request", 2, error.to_string(), false)
            });
            let task = repository.reopen(task_id, status, &context.actor).unwrap_or_else(|error| fail_store(json_output, error));
            cli_success(json_output, serde_json::to_value(&task).unwrap(), format!("(#{}) reopened.", task.id));
        }
        Some(Commands::Depends(args)) => match args.command {
            DependsCommands::Add { id, depends_on } => {
                let task_id = parse_cli_task_id(&id, json_output);
                let dependency_id = parse_cli_task_id(&depends_on, json_output);
                let repository = repository_for_task(&context, task_id, json_output);
                repository.add_dependency(task_id, dependency_id, &context.actor).unwrap_or_else(|error| fail_store(json_output, error));
                let task = repository.get(task_id).unwrap_or_else(|error| fail_store(json_output, error));
                cli_success(json_output, serde_json::to_value(&task).unwrap(), format!("Task #{} now depends on #{}.", task_id, dependency_id));
            }
            DependsCommands::Remove { id, depends_on } => {
                let task_id = parse_cli_task_id(&id, json_output);
                let dependency_id = parse_cli_task_id(&depends_on, json_output);
                let repository = repository_for_task(&context, task_id, json_output);
                repository.remove_dependency(task_id, dependency_id, &context.actor).unwrap_or_else(|error| fail_store(json_output, error));
                let task = repository.get(task_id).unwrap_or_else(|error| fail_store(json_output, error));
                cli_success(json_output, serde_json::to_value(&task).unwrap(), format!("Dependency #{} removed from task #{}.", dependency_id, task_id));
            }
            DependsCommands::List { id } => {
                let task_id = parse_cli_task_id(&id, json_output);
                let repository = repository_for_task(&context, task_id, json_output);
                let dependencies = repository.list_dependencies(task_id).unwrap_or_else(|error| fail_store(json_output, error));
                cli_success(json_output, serde_json::to_value(&dependencies).unwrap(), format!("Task #{} dependencies: {}", task_id, dependencies.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")));
            }
        },
        Some(Commands::History(args)) => {
            let task_id = parse_cli_task_id(&args.id, json_output);
            let repository = repository_for_task(&context, task_id, json_output);
            let mut events = repository.get_history(task_id).unwrap_or_else(|error| fail_store(json_output, error));
            events.reverse();
            events.truncate(args.limit);
            cli_success(json_output, serde_json::to_value(&events).unwrap(), format!("Task #{}: {} history events", task_id, events.len()));
        }
        Some(Commands::Lock(args)) => match args.command {
            LockCommands::Acquire { id } => {
                let task_id = parse_cli_task_id(&id, json_output);
                let owner = new_lock_owner_id(json_output);
                let grant = context.lock_service.acquire(task_id, &owner, &context.actor)
                    .unwrap_or_else(|error| fail_store(json_output, error));
                cli_success(json_output, lock_grant_data(&grant), format!("Lock acquired for task #{}; save the returned lock_token.", task_id));
            }
            LockCommands::Recover { id } => {
                let task_id = parse_cli_task_id(&id, json_output);
                let owner = new_lock_owner_id(json_output);
                let grant = context.lock_service.recover(task_id, &owner, &context.actor)
                    .unwrap_or_else(|error| fail_store(json_output, error));
                cli_success(json_output, lock_grant_data(&grant), format!("Lock recovered for task #{}; save the returned lock_token.", task_id));
            }
            LockCommands::Heartbeat { id, owner_id, fencing_generation } => {
                let task_id = parse_cli_task_id(&id, json_output);
                let token = std::env::var("TODOING_LOCK_TOKEN").unwrap_or_else(|_| {
                    cli_failure(json_output, "stale_lock", 4, "TODOING_LOCK_TOKEN is required", false)
                });
                let grant = context.lock_service.grant_from_token(task_id, &token)
                    .unwrap_or_else(|error| fail_store(json_output, error));
                if grant.owner_id != owner_id || grant.fencing_generation != fencing_generation {
                    cli_failure(json_output, "stale_lock", 4, "Lock owner or fencing generation does not match", false);
                }
                let renewed = context.lock_service.heartbeat(&grant, &context.actor)
                    .unwrap_or_else(|error| fail_store(json_output, error));
                cli_success(json_output, serde_json::json!({"heartbeat_at": renewed.heartbeat_at, "expires_at": renewed.expires_at}), format!("Lock heartbeat renewed for task #{}.", task_id));
            }
            LockCommands::Release { id, owner_id, fencing_generation } => {
                let task_id = parse_cli_task_id(&id, json_output);
                let token = std::env::var("TODOING_LOCK_TOKEN").unwrap_or_else(|_| {
                    cli_failure(json_output, "stale_lock", 4, "TODOING_LOCK_TOKEN is required", false)
                });
                let grant = context.lock_service.grant_from_token(task_id, &token)
                    .unwrap_or_else(|error| fail_store(json_output, error));
                if grant.owner_id != owner_id || grant.fencing_generation != fencing_generation {
                    cli_failure(json_output, "stale_lock", 4, "Lock owner or fencing generation does not match", false);
                }
                context.lock_service.release(&grant, &context.actor)
                    .unwrap_or_else(|error| fail_store(json_output, error));
                cli_success(json_output, serde_json::json!({"released": true, "fencing_generation": grant.fencing_generation}), format!("Lock released for task #{}.", task_id));
            }
        },
        Some(Commands::Config) => {
            let text = context.config.to_toml().unwrap_or_else(|error| cli_failure(json_output, "config_invalid", 2, error.to_string(), false));
            cli_success(json_output, serde_json::to_value(&context.config).unwrap(), text);
        }
        Some(Commands::Verify) => {
            let report = context.store.verify().unwrap_or_else(|error| fail_store(json_output, error));
            cli_success(json_output, serde_json::to_value(&report).unwrap(), "SQLite integrity check passed.");
        }
        Some(Commands::Snapshot(args)) => {
            let report = context.store.snapshot(args.path, args.overwrite)
                .unwrap_or_else(|error| fail_store(json_output, error));
            cli_success(json_output, serde_json::to_value(&report).unwrap(), format!("Snapshot written: {} ({} bytes)", report.path.display(), report.size_bytes));
        }
        Some(Commands::Hooks(args)) => match args.command {
            HooksCommands::Status => show_hook_status(json_output),
            HooksCommands::Install { hook } => install_hooks(hook, json_output),
        },
        Some(Commands::InternalHook(args)) => run_internal_hook(&args.name).unwrap_or_else(|error| cli_failure(json_output, "invalid_request", 2, error, false)),
        Some(Commands::Api) | Some(Commands::Init(_)) | Some(Commands::Migrate(_)) | None => unreachable!(),
    }
}

fn run() {
    let cli = Cli::parse();
    let database_exists = std::path::Path::new(".todoing/todoing.db").is_file();
    let forced_sqlite = matches!(&cli.command,
        Some(Commands::Api) | Some(Commands::Init(_)) | Some(Commands::Migrate(_)) |
        Some(Commands::Complete(_)) | Some(Commands::Cancel(_)) |
        Some(Commands::Reopen(_)) | Some(Commands::Depends(_)) |
        Some(Commands::History(_)) | Some(Commands::Lock(_)) |
        Some(Commands::Config) | Some(Commands::Verify) |
        Some(Commands::Snapshot(_)) |
        Some(Commands::Version(_))
    );
    let legacy_only = matches!(&cli.command, Some(Commands::Hooks(_)) | Some(Commands::InternalHook(_)));
    if database_exists && !legacy_only || forced_sqlite {
        run_sqlite(cli);
    } else {
        run_legacy(cli);
    }
}



#[cfg(test)]
mod sqlite_cli_tests {
    use super::*;

    fn sample_task() -> SqliteTask {
        SqliteTask {
            id: TaskId::new(17).unwrap(),
            title: "CLI task".into(),
            status_id: StatusId::new("todo").unwrap(),
            outcome: None,
            body: "original body".into(),
            created_at: "2026-11-21T10:00:00Z".into(),
            updated_at: "2026-11-21T10:00:00Z".into(),
            archived_at: None,
            archived_by: None,
            labels: vec!["alpha".into(), "beta".into()],
            commit_refs: vec!["0123456789abcdef0123456789abcdef01234567".into()],
            dependencies: vec![],
        }
    }

    #[test]
    fn sqlite_edit_markdown_round_trips_supported_fields() {
        let original = sample_task();
        let markdown = format_sqlite_see(&original);
        let edited = parse_edited_task(&markdown, &original, false);
        assert_eq!(edited.id, original.id);
        assert_eq!(edited.title, original.title);
        assert_eq!(edited.status_id, original.status_id);
        assert_eq!(edited.body, original.body);
        assert_eq!(edited.labels, original.labels);
        assert_eq!(edited.commit_refs, original.commit_refs);
        assert_eq!(edited.created_at, original.created_at);
    }

    #[test]
    fn sqlite_markdown_edit_can_change_status_and_body_together() {
        let original = sample_task();
        let markdown = format_sqlite_see(&original)
            .replace("status: todo", "status: in_progress")
            .replace("original body", "revised body");
        let edited = parse_edited_task(&markdown, &original, false);
        assert_eq!(edited.status_id.as_str(), "in_progress");
        assert_eq!(edited.body, "revised body");
        assert_eq!(edited.outcome, None);
    }

    #[test]
    fn sqlite_list_format_includes_task_identity_and_status() {
        let task = sample_task();
        let line = format_sqlite_list(&task);
        assert!(line.contains("(#17)"));
        assert!(line.contains("alpha,beta"));
        assert!(line.contains("CLI task"));
    }
}

fn main() { run(); }
