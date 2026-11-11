use clap::{Args, Parser, Subcommand};
use serde::Serialize;
use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};
use todoing::{Store, Task, TaskStatus, VERSION};

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
    Add(AddArgs),
    #[command(name="ls")]
    Ls(ListArgs),
    See(IdArg),
    Edit(IdArg),
    Body(TextArgs),
    Append(TextArgs),
    Status(StatusArgs),
    Label(LabelArgs),
    Commit(CommitArgs),
    Delete(DeleteArgs),
    Reindex,
    Version,
}

#[derive(Args)]
struct AddArgs {
    title: String,
    #[arg(short='l', long="label")]
    labels: Vec<String>,
    #[arg(short='s', long="status", default_value="todo")]
    status: String,
    #[arg(short='b', long="body", default_value="")]
    body: String,
}
#[derive(Args)]
struct ListArgs {
    #[arg(short='n', long="limit", default_value_t=0, allow_negative_numbers=true)]
    limit: i64,
    #[arg(short='s', long="status")]
    status: Option<String>,
    #[arg(short='l', long="label")]
    labels: Vec<String>,
    #[arg(short='q', long="query")]
    query: Option<String>,
    #[arg(short='r', long="reverse")]
    reverse: bool,
    #[arg(long="scan")]
    scan: bool,
}
#[derive(Args)]
struct IdArg {
    id: String,
}
#[derive(Args)]
struct TextArgs {
    id: String,
    text: String,
}
#[derive(Args)]
struct StatusArgs {
    id: String,
    new_status: String,
}
#[derive(Args)]
struct LabelArgs {
    id: String,
    #[arg(required=true, num_args=1..)]
    ops: Vec<String>,
    #[arg(long="clear", action=clap::ArgAction::SetTrue, overrides_with="no_clear")]
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
    #[arg(short='f', long="force")]
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

fn validate_before_done(task: &Task) -> Result<(), String> {
    if task.commits.is_empty() {
        return Err("Cannot mark task done: link at least one Git commit first.".into());
    }
    for sha in &task.commits {
        let spec = format!("{}^{{commit}}", sha);
        let valid = Command::new("git").arg("cat-file").arg("-e").arg(&spec).output()
            .map(|output| output.status.success()).unwrap_or(false);
        if !valid {
            return Err(format!("Linked Git commit '{}' is not available in this repository.", sha));
        }
    }

    let changes = PathBuf::from("openspec").join("changes");
    if changes.is_dir() {
        let entries = fs::read_dir(&changes).map_err(|e| e.to_string())?;
        for entry in entries.flatten().filter(|e| e.path().is_dir()) {
            if entry.file_name().to_string_lossy() == "archive" { continue; }
            let tasks_file = entry.path().join("tasks.md");
            if !tasks_file.is_file() { continue; }
            let contents = fs::read_to_string(&tasks_file).map_err(|e| e.to_string())?;
            if contents.lines().any(|line| {
                let line = line.trim_start();
                line.starts_with("- [ ]") || line.starts_with("* [ ]")
            }) {
                return Err(format!("Cannot mark task done: OpenSpec tasks remain unchecked in {}.", tasks_file.display()));
            }
        }
    }
    Ok(())
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
                validate_before_done(&task).unwrap_or_else(|e| fail(e));
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
                print!("Delete (#{}) {}? [y/N]: ", id, task.title);
                let _ = std::io::stdout().flush();
                let mut answer = String::new();
                let bytes_read = std::io::stdin().read_line(&mut answer).unwrap_or_else(|_| {
                    eprintln!("Aborted!");
                    std::process::exit(1);
                });
                if bytes_read == 0 {
                    eprintln!("Aborted!");
                    std::process::exit(1);
                }
                if !matches!(answer.trim().to_lowercase().as_str(), "y" | "yes") { println!("Aborted."); return; }
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
        None => {}
    }
}
fn main() { run(); }
