use clap::{Args, Parser, Subcommand};
use serde::Serialize;
use std::{
    fs,
    io::{Read, Write},
    path::PathBuf,
    process::{Command, Stdio},
};
use tempfile::NamedTempFile;
use todoing::{Store, Task, TaskStatus, VERSION};

const GIT_HOOK_UNIX: &str = include_str!("../hooks/before-done-git");
const OPENSPEC_HOOK_UNIX: &str = include_str!("../hooks/before-done-openspec");
const GIT_HOOK_WINDOWS: &str = include_str!("../hooks/before-done-git.cmd");
const OPENSPEC_HOOK_WINDOWS: &str = include_str!("../hooks/before-done-openspec.cmd");

#[derive(Parser)]
#[command(name="todoing", version=VERSION, about="Local, git-friendly task manager for AI agents and humans", disable_help_subcommand=true, arg_required_else_help=true)]
struct Cli {
    #[arg(long, global=true, help="Emit machine-readable JSON")]
    json: bool,
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
    #[command(about="Show the installed todoing version.")]
    Version,
    #[command(about="Manage optional standard hooks.")]
    Hooks(HooksArgs),
    #[command(name="__hook", hide=true)]
    InternalHook(InternalHookArgs),
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

fn fail(message: impl AsRef<str>) -> ! {
    eprintln!("Error: {}", message.as_ref());
    std::process::exit(1)
}
fn task_or_fail(store: &Store, raw: &str) -> (i64, Task) {
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
fn emit_task(task: &Task, json: bool, human: String) {
    if json { emit_json(task); } else { println!("{}", human); }
}

fn run_hook(name: &str, task: &Task) -> Result<(), String> {
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
    child.stdin.take().ok_or_else(|| "Hook stdin unavailable".to_string())?
        .write_all(&payload).map_err(|e| e.to_string())?;
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
        emit_json(&serde_json::json!({
            "hooks": rows,
            "agent_action": if missing { "ask_user_before_install" } else { "none" }
        }));
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
        emit_json(&serde_json::json!({
            "installed": installed,
            "already_installed": already_installed
        }));
    } else {
        for name in installed { println!("Installed {}.", name); }
        for name in already_installed { println!("{} is already present; left unchanged.", name); }
    }
}

fn run_internal_hook(name: &str) -> Result<(), String> {
    let mut payload = Vec::new();
    std::io::stdin().read_to_end(&mut payload).map_err(|e| format!("Cannot read task data: {}", e))?;
    let task: Task = serde_json::from_slice(&payload).map_err(|e| format!("Invalid task data: {}", e))?;
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

fn run() {
    let cli = Cli::parse();
    let json = cli.json;
    let store = Store::current();
    match cli.command {
        Some(Commands::Add(a)) => {
            let status = TaskStatus::parse(&a.status).unwrap_or_else(|e| fail(e));
            let task = Task::new(store.next_id(), a.title, status, a.labels, a.body);
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
        Some(Commands::Delete(a)) => {
            let (id, task) = task_or_fail(&store, &a.id);
            if !a.force {
                if json {
                    eprint!("Delete (#{}) {}? [y/N]: ", id, task.title);
                    let _ = std::io::stderr().flush();
                } else {
                    print!("Delete (#{}) {}? [y/N]: ", id, task.title);
                    let _ = std::io::stdout().flush();
                }
                let mut answer = String::new();
                let bytes_read = std::io::stdin().read_line(&mut answer).unwrap_or_else(|_| {
                    eprintln!("Aborted!");
                    std::process::exit(1);
                });
                if bytes_read == 0 {
                    eprintln!("Aborted!");
                    std::process::exit(1);
                }
                if !matches!(answer.trim().to_lowercase().as_str(), "y" | "yes") {
                    if json { emit_json(&serde_json::json!({"id": id, "deleted": false})); }
                    else { println!("Aborted."); }
                    return;
                }
            }
            store.delete_task_file(id).unwrap_or_else(|e| fail(e));
            store.sync_delete(id).unwrap_or_else(|e| fail(e));
            if json { emit_json(&serde_json::json!({"id": id, "deleted": true})); }
            else { println!("(#{}) deleted.", id); }
        }
        Some(Commands::Reindex) => {
            let tasks = store.rebuild_index().unwrap_or_else(|e| fail(e));
            if json { emit_json(&tasks); } else { println!("Index rebuilt: {} tasks.", tasks.len()); }
        }
        Some(Commands::Version) => {
            if json { emit_json(&serde_json::json!({"version": VERSION})); }
            else { println!("{}", VERSION); }
        }
        Some(Commands::Hooks(args)) => match args.command {
            HooksCommands::Status => show_hook_status(json),
            HooksCommands::Install { hook } => install_hooks(hook, json),
        },
        Some(Commands::InternalHook(args)) => run_internal_hook(&args.name).unwrap_or_else(|e| fail(e)),
        None => {}
    }
}
fn main() { run(); }
