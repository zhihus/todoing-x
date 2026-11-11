use clap::{Args, Parser, Subcommand};
use std::process::Command;
use todoing::{Store, Task, TaskStatus, VERSION};

#[derive(Parser)]
#[command(name="todoing", version=VERSION, about="Local, git-friendly task manager for AI agents and humans", disable_help_subcommand=true, no_args_is_help=true)]
struct Cli { #[command(subcommand)] command: Option<Commands> }

#[derive(Subcommand)]
enum Commands {
    Add(AddArgs),
    #[command(name="ls")] Ls(ListArgs),
    See(IdArg),
    Edit(IdArg),
    Body(TextArgs),
    Append(TextArgs),
    Status(StatusArgs),
    Label(LabelArgs),
    Delete(DeleteArgs),
    Reindex,
    Version,
}
#[derive(Args)] struct AddArgs {
    title: String,
    #[arg(short='l', long="label")] labels: Vec<String>,
    #[arg(short='s', long="status", default_value="todo")] status: String,
    #[arg(short='b', long="body", default_value="")] body: String,
}
#[derive(Args)] struct ListArgs {
    #[arg(short='n', long="limit", default_value_t=0)] limit: usize,
    #[arg(short='s', long="status")] status: Option<String>,
    #[arg(short='l', long="label")] labels: Vec<String>,
    #[arg(short='q', long="query")] query: Option<String>,
    #[arg(short='r', long="reverse")] reverse: bool,
    #[arg(long="scan")] scan: bool,
}
#[derive(Args)] struct IdArg { id: String }
#[derive(Args)] struct TextArgs { id: String, text: String }
#[derive(Args)] struct StatusArgs { id: String, new_status: String }
#[derive(Args)] struct LabelArgs {
    id: String,
    #[arg(required=true, num_args=1..)] ops: Vec<String>,
    #[arg(long="clear")] clear: bool,
}
#[derive(Args)] struct DeleteArgs {
    id: String,
    #[arg(short='f', long="force")] force: bool,
}

fn fail(message: impl AsRef<str>) -> ! { eprintln!("Error: {}", message.as_ref()); std::process::exit(1) }
fn task_or_fail(store: &Store, raw: &str) -> (i64, Task) {
    let id = Store::resolve_id(raw).unwrap_or_else(|e| fail(e));
    let task = store.read_task(id).unwrap_or_else(|e| fail(e)).unwrap_or_else(|| fail(format!("Task #{} not found.", id)));
    (id, task)
}
fn run() {
    let cli = Cli::parse();
    let store = Store::current();
    match cli.command {
        Some(Commands::Add(a)) => {
            let status = TaskStatus::parse(&a.status).unwrap_or_else(|e| fail(e));
            let task = Task::new(store.next_id(), a.title, status, a.labels, a.body);
            store.write_task(&task).unwrap_or_else(|e| fail(e));
            store.sync_mutation(&task).unwrap_or_else(|e| fail(e));
            println!("(#{}) {}", task.id, task.title);
        }
        Some(Commands::Ls(a)) => {
            if a.scan { store.rebuild_index().unwrap_or_else(|e| fail(e)); }
            let status = a.status.as_deref().map(TaskStatus::parse).transpose().unwrap_or_else(|e| fail(e));
            let tasks = store.filter(status, &a.labels, a.limit, a.query.as_deref(), a.reverse).unwrap_or_else(|e| fail(e));
            if tasks.is_empty() { println!("No tasks."); }
            else { for t in tasks { println!("{}", t.format_list()); } }
        }
        Some(Commands::See(a)) => {
            let (_, task) = task_or_fail(&store, &a.id);
            print!("{}", task.format_see());
        }
        Some(Commands::Edit(a)) => {
            let (id, _) = task_or_fail(&store, &a.id);
            let path = std::env::current_dir().unwrap_or_default().join(".todoing/tasks").join(format!("{}.md", id));
            let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".into());
            let status = Command::new(editor).arg(path).status().unwrap_or_else(|e| fail(e.to_string()));
            if !status.success() { fail(format!("Editor exited with code {}", status.code().unwrap_or(1))); }
            store.rebuild_index().unwrap_or_else(|e| fail(e));
        }
        Some(Commands::Body(a)) => {
            let (_, mut task) = task_or_fail(&store, &a.id);
            task.body = a.text; store.write_task(&task).unwrap_or_else(|e| fail(e));
            store.sync_mutation(&task).unwrap_or_else(|e| fail(e));
            println!("(#{}) body updated.", task.id);
        }
        Some(Commands::Append(a)) => {
            let (_, mut task) = task_or_fail(&store, &a.id);
            if task.body.is_empty() { task.body = a.text; } else { task.body = format!("{}\n\n{}", task.body, a.text); }
            store.write_task(&task).unwrap_or_else(|e| fail(e));
            store.sync_mutation(&task).unwrap_or_else(|e| fail(e));
            println!("(#{}) body appended.", task.id);
        }
        Some(Commands::Status(a)) => {
            let (_, mut task) = task_or_fail(&store, &a.id);
            task.status = TaskStatus::parse(&a.new_status).unwrap_or_else(|e| fail(e));
            store.write_task(&task).unwrap_or_else(|e| fail(e));
            store.sync_mutation(&task).unwrap_or_else(|e| fail(e));
            println!("(#{}) status -> {{{}}} {}", task.id, task.status.icon(), task.status.as_str());
        }
        Some(Commands::Label(a)) => {
            let (_, mut task) = task_or_fail(&store, &a.id);
            let mut current = if a.clear { Vec::new() } else { task.labels.clone() };
            let mut adds = Vec::new(); let mut removes = Vec::new();
            for op in a.ops {
                if let Some(v) = op.strip_prefix('+') { adds.push(v.to_string()); }
                else if let Some(v) = op.strip_prefix('-') { removes.push(v.to_string()); }
                else { fail(format!("Invalid label operation '{}'. Use +label to add or -label to remove.", op)); }
            }
            for r in removes { if let Some(pos) = current.iter().position(|x| x == &r) { current.remove(pos); } }
            current.extend(adds);
            let mut seen = std::collections::HashSet::new();
            task.labels = current.into_iter().filter(|x| seen.insert(x.clone())).collect();
            store.write_task(&task).unwrap_or_else(|e| fail(e));
            store.sync_mutation(&task).unwrap_or_else(|e| fail(e));
            println!("(#{}) labels -> [{}]", task.id, task.labels.join(", "));
        }
        Some(Commands::Delete(a)) => {
            let (id, task) = task_or_fail(&store, &a.id);
            if !a.force {
                eprint!("Delete (#{}) {}? [y/N] ", id, task.title);
                let mut answer = String::new(); let _ = std::io::stdin().read_line(&mut answer);
                if !matches!(answer.trim().to_lowercase().as_str(), "y" | "yes") { println!("Aborted."); return; }
            }
            store.delete_task_file(id).unwrap_or_else(|e| fail(e));
            store.sync_delete(id).unwrap_or_else(|e| fail(e));
            println!("(#{}) deleted.", id);
        }
        Some(Commands::Reindex) => {
            let tasks = store.rebuild_index().unwrap_or_else(|e| fail(e));
            println!("Index rebuilt: {} tasks.", tasks.len());
        }
        Some(Commands::Version) => println!("{}", VERSION),
        None => {}
    }
}
fn main() { run(); }
