use crate::{
    config::ProjectConfig,
    domain::{ActorContext, ActorId, ProjectId, StatusId, TaskId},
    storage::{LockGrant, NewTask, Store, StoreError, TaskRepository},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{fs, io::{self, Read}, path::Path, process::Command};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    api_version: u32,
    #[serde(default)]
    request_id: Option<String>,
    project_id: String,
    actor_id: String,
    operation: String,
    args: Value,
    #[serde(default)]
    lock_token: Option<String>,
}

#[derive(Serialize)]
struct Success {
    api_version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    request_id: Option<String>,
    ok: bool,
    data: Value,
}

#[derive(Serialize)]
struct Failure {
    api_version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    request_id: Option<String>,
    ok: bool,
    error: ApiError,
}

#[derive(Debug, Serialize)]
struct ApiError {
    code: &'static str,
    message: String,
    retryable: bool,
    details: Value,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyArgs {}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateArgs {
    title: String,
    #[serde(default)]
    status_id: Option<String>,
    #[serde(default)]
    body: String,
    #[serde(default)]
    labels: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskIdArgs { task_id: i64 }

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ListArgs {
    #[serde(default)]
    status_id: Option<String>,
    #[serde(default)]
    outcome: Option<String>,
    #[serde(default)]
    labels: Vec<String>,
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    reverse: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateArgs { task_id: i64, patch: UpdatePatch }

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdatePatch {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    body: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StatusArgs { task_id: i64, status_id: String }

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CancelArgs {
    task_id: i64,
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReopenArgs {
    task_id: i64,
    #[serde(default)]
    target_status_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DependencyArgs { task_id: i64, depends_on_task_id: i64 }

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct VersionArgs { task_id: i64, version_no: i64 }

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct VersionListArgs {
    task_id: i64,
    #[serde(default)]
    before_version_no: Option<i64>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoryArgs {
    task_id: i64,
    #[serde(default)]
    before_event_id: Option<i64>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LabelArgs { task_id: i64, label: String }

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommitArgs { task_id: i64, revision: String }

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CompareArgs { task_id: i64, from_version_no: i64, to_version_no: i64 }

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LockArgs { task_id: i64 }

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LockOwnerArgs {
    task_id: i64,
    owner_id: String,
    fencing_generation: i64,
}

fn parse_args<T: DeserializeOwned>(value: Value) -> Result<T, String> {
    serde_json::from_value(value).map_err(|error| format!("Invalid operation arguments: {error}"))
}

fn id(raw: i64) -> Result<TaskId, String> {
    TaskId::new(raw).map_err(|error| error.to_string())
}

fn store_error(error: StoreError) -> ApiError {
    let (code, retryable) = match &error {
        StoreError::TaskNotFound(_) => ("task_not_found", false),
        StoreError::TaskTerminal => ("task_terminal", false),
        StoreError::TaskLocked => ("task_locked", true),
        StoreError::LockExpired => ("lock_expired", false),
        StoreError::StaleLock => ("stale_lock", false),
        StoreError::InvalidStatusTransition(_) => ("invalid_status_transition", false),
        StoreError::DependencyNotFound(_) => ("dependency_not_found", false),
        StoreError::DependencyCycle => ("dependency_cycle", false),
        StoreError::VersionNotFound { .. } => ("version_not_found", false),
        StoreError::ProjectIdMismatch { .. } => ("project_id_mismatch", false),
        StoreError::InvalidConfig(_) => ("config_invalid", false),
        StoreError::HistoryRecordFailed(_) => ("history_record_failed", false),
        StoreError::AlreadyExists(_) => ("migration_conflict", false),
        StoreError::IntegrityCheckFailed(_) | StoreError::ForeignKeyViolations(_) |
        StoreError::MigrationChecksumMismatch { .. } | StoreError::InvalidMetadata(_) |
        StoreError::MissingMetadata(_) | StoreError::UnsupportedSchemaVersion(_) => ("store_corrupt", false),
        StoreError::Sqlite(rusqlite::Error::SqliteFailure(code, _))
            if code.code == rusqlite::ErrorCode::DatabaseBusy ||
               code.code == rusqlite::ErrorCode::DatabaseLocked => ("store_busy", true),
        StoreError::Io(_) => ("io_error", false),
        _ => ("invalid_request", false),
    };
    ApiError { code, message: error.to_string(), retryable, details: json!({}) }
}

fn execute(request: Request, root: &Path) -> Result<Value, ApiError> {
    if request.api_version != 1 {
        return Err(ApiError {
            code: "unsupported_api_version",
            message: format!("Unsupported API version: {}", request.api_version),
            retryable: false,
            details: json!({"supported": [1]}),
        });
    }
    if request.request_id.as_ref().is_some_and(|value| value.is_empty() || value.len() > 128) {
        return Err(ApiError { code: "invalid_request", message: "request_id must contain 1 to 128 characters".into(), retryable: false, details: json!({}) });
    }
    const OPERATIONS: &[&str] = &[
        "store.info", "store.verify", "store.migrate", "store.snapshot",
        "task.create", "task.list", "task.get", "task.update", "task.status.set",
        "task.complete", "task.cancel", "task.reopen", "task.label.add",
        "task.label.remove", "task.commit.add", "task.dependency.add",
        "task.dependency.remove", "task.dependency.list", "task.history.list",
        "task.version.list", "task.version.get", "task.version.compare",
        "task.version.approve", "task.lock.acquire", "task.lock.heartbeat",
        "task.lock.release", "task.lock.recover",
    ];
    if !OPERATIONS.contains(&request.operation.as_str()) {
        return Err(invalid(format!("Unknown operation '{}'", request.operation)));
    }
    let project_id = ProjectId::new(request.project_id).map_err(|e| ApiError { code: "invalid_request", message: e.to_string(), retryable: false, details: json!({}) })?;
    let actor_id = ActorId::new(request.actor_id).map_err(|e| ApiError { code: "invalid_request", message: e.to_string(), retryable: false, details: json!({}) })?;
    let actor = ActorContext::new(actor_id);
    let config_path = root.join(".todoing").join("config.toml");
    let config_text = fs::read_to_string(&config_path).map_err(|error| ApiError { code: "config_invalid", message: format!("Cannot read {}: {error}", config_path.display()), retryable: false, details: json!({}) })?;
    let config = ProjectConfig::from_toml(&config_text).map_err(|error| ApiError { code: "config_invalid", message: error.to_string(), retryable: false, details: json!({}) })?;
    let store = Store::open(root, Some(&project_id)).map_err(store_error)?;
    let mut repository = TaskRepository::new(store.clone(), config.clone()).map_err(store_error)?;
    let lock_service = repository.lock_service().map_err(store_error)?;
    if let Some(token) = request.lock_token.as_deref() {
        let raw_task_id = request.args.get("task_id").and_then(Value::as_i64)
            .ok_or_else(|| invalid("lock_token requires task_id in args".into()))?;
        let grant = lock_service.grant_from_token(id(raw_task_id).map_err(|error| invalid(error.to_string()))?, token).map_err(store_error)?;
        repository = repository.with_lock_grant(&grant);
    }

    match request.operation.as_str() {
        "store.info" => {
            parse_args::<EmptyArgs>(request.args).map_err(|error| invalid(error.to_string()))?;
            let connection = store.connection().map_err(store_error)?;
            let task_count: i64 = connection.query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get(0))
                .map_err(|error| store_error(StoreError::from(error)))?;
            Ok(json!({"project_id": store.project_id(), "schema_version": 1, "api_version": 1, "task_count": task_count}))
        }
        "store.verify" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Args { #[serde(default = "default_true")] deep: bool }
            fn default_true() -> bool { true }
            let args: Args = parse_args(request.args).map_err(|error| invalid(error.to_string()))?;
            if args.deep {
                let report = store.verify().map_err(store_error)?;
                Ok(json!({"integrity_check": report.integrity_check, "foreign_key_violations": report.foreign_key_violations, "schema_version": report.schema_version}))
            } else {
                Ok(json!({"integrity_check": "skipped", "foreign_key_violations": 0, "schema_version": 1}))
            }
        }
        "task.create" => {
            let args: CreateArgs = parse_args(request.args).map_err(|error| invalid(error.to_string()))?;
            let status = match args.status_id {
                Some(value) => StatusId::new(value).map_err(|error| invalid(error.to_string()))?,
                None => StatusId::new(config.first_enabled_status().ok_or_else(|| invalid("No enabled statuses".into()))?.id.clone()).map_err(|error| invalid(error.to_string()))?,
            };
            let task = repository.create(NewTask { title: args.title, status_id: status, outcome: None, body: args.body, labels: args.labels, commit_refs: vec![] }, &actor).map_err(store_error)?;
            let event_id = repository.get_history(task.id).map_err(store_error)?.last().map(|event| event.event_id);
            Ok(json!({"task": task, "version_no": 1, "event_id": event_id}))
        }
        "task.list" => {
            let args: ListArgs = parse_args(request.args).map_err(|error| invalid(error.to_string()))?;
            if args.limit == Some(0) { return Err(invalid("limit must be a positive integer".into())); }
            let mut tasks = repository.list().map_err(store_error)?;
            if let Some(status) = args.status_id { tasks.retain(|task| task.status_id.as_str() == status); }
            if let Some(outcome) = args.outcome {
                if !matches!(outcome.as_str(), "active" | "completed" | "cancelled") {
                    return Err(invalid("outcome must be active, completed, or cancelled".into()));
                }
                tasks.retain(|task| match outcome.as_str() {
                    "active" => task.outcome.is_none(),
                    "completed" => task.outcome.is_some_and(|value| value == crate::domain::TaskOutcome::Completed),
                    "cancelled" => task.outcome.is_some_and(|value| value == crate::domain::TaskOutcome::Cancelled),
                    _ => unreachable!(),
                });
            }
            for label in args.labels { tasks.retain(|task| task.labels.iter().any(|item| item.eq_ignore_ascii_case(&label))); }
            if let Some(query) = args.query {
                let query = query.to_lowercase();
                tasks.retain(|task| task.title.to_lowercase().contains(&query) || task.body.to_lowercase().contains(&query) || task.labels.iter().any(|label| label.to_lowercase().contains(&query)) || task.status_id.as_str() == query);
            }
            tasks.sort_by_key(|task| task.id);
            if !args.reverse { tasks.reverse(); }
            if let Some(limit) = args.limit { tasks.truncate(limit); }
            Ok(json!({"tasks": tasks}))
        }
        "task.get" => {
            let args: TaskIdArgs = parse_args(request.args).map_err(|error| invalid(error.to_string()))?;
            let task = repository.get(id(args.task_id).map_err(|error| invalid(error.to_string()))?).map_err(store_error)?;
            let version_no = repository.list_versions(task.id).map_err(store_error)?.last().map(|version| version.version_no).unwrap_or(0);
            let baseline = repository.get_baseline(task.id).map_err(store_error)?;
            Ok(json!({"task": task, "version_no": version_no, "baseline_version_no": baseline.map(|value| value.version_no)}))
        }
        "task.update" => {
            let args: UpdateArgs = parse_args(request.args).map_err(|error| invalid(error.to_string()))?;
            if args.patch.title.is_none() && args.patch.body.is_none() { return Err(invalid("patch must contain title and/or body".into())); }
            let task_id = id(args.task_id).map_err(|error| invalid(error.to_string()))?;
            let mut task = repository.get(task_id).map_err(store_error)?;
            if let Some(title) = args.patch.title { task.title = title; }
            if let Some(body) = args.patch.body { task.body = body; }
            let updated = repository.update(&task, &actor).map_err(store_error)?;
            Ok(json!({"task": updated}))
        }
        "task.status.set" => {
            let args: StatusArgs = parse_args(request.args).map_err(|error| invalid(error.to_string()))?;
            let status = StatusId::new(args.status_id).map_err(|error| invalid(error.to_string()))?;
            let task = repository.set_status(id(args.task_id).map_err(|error| invalid(error.to_string()))?, status, &actor).map_err(store_error)?;
            Ok(json!({"task": task}))
        }
        "task.complete" => {
            let args: TaskIdArgs = parse_args(request.args).map_err(|error| invalid(error.to_string()))?;
            Ok(json!({"task": repository.complete(id(args.task_id).map_err(|error| invalid(error.to_string()))?, &actor).map_err(store_error)?}))
        }
        "task.cancel" => {
            let args: CancelArgs = parse_args(request.args).map_err(|error| invalid(error.to_string()))?;
            Ok(json!({"task": repository.cancel(id(args.task_id).map_err(|error| invalid(error.to_string()))?, args.reason.as_deref(), &actor).map_err(store_error)?}))
        }
        "task.reopen" => {
            let args: ReopenArgs = parse_args(request.args).map_err(|error| invalid(error.to_string()))?;
            let target = args.target_status_id.map(StatusId::new).transpose().map_err(|error| invalid(error.to_string()))?;
            Ok(json!({"task": repository.reopen(id(args.task_id).map_err(|error| invalid(error.to_string()))?, target, &actor).map_err(store_error)?}))
        }
        "task.dependency.add" => {
            let args: DependencyArgs = parse_args(request.args).map_err(|error| invalid(error.to_string()))?;
            let task_id = id(args.task_id).map_err(|error| invalid(error.to_string()))?;
            repository.add_dependency(task_id, id(args.depends_on_task_id).map_err(|error| invalid(error.to_string()))?, &actor).map_err(store_error)?;
            Ok(json!({"task": repository.get(task_id).map_err(store_error)?}))
        }
        "task.dependency.remove" => {
            let args: DependencyArgs = parse_args(request.args).map_err(|error| invalid(error.to_string()))?;
            let task_id = id(args.task_id).map_err(|error| invalid(error.to_string()))?;
            repository.remove_dependency(task_id, id(args.depends_on_task_id).map_err(|error| invalid(error.to_string()))?, &actor).map_err(store_error)?;
            Ok(json!({"task": repository.get(task_id).map_err(store_error)?}))
        }
        "task.dependency.list" => {
            let args: TaskIdArgs = parse_args(request.args).map_err(|error| invalid(error.to_string()))?;
            Ok(json!({"dependencies": repository.list_dependencies(id(args.task_id).map_err(|error| invalid(error.to_string()))?).map_err(store_error)?}))
        }
        "task.history.list" => {
            let args: HistoryArgs = parse_args(request.args).map_err(|error| invalid(error.to_string()))?;
            if args.limit == Some(0) { return Err(invalid("limit must be a positive integer".into())); }
            let mut events = repository.get_history(id(args.task_id).map_err(|error| invalid(error.to_string()))?).map_err(store_error)?;
            events.reverse();
            if let Some(cursor) = args.before_event_id { events.retain(|event| event.event_id < cursor); }
            events.truncate(args.limit.unwrap_or(100));
            Ok(json!({"events": events}))
        }
        "task.version.list" => {
            let args: VersionListArgs = parse_args(request.args).map_err(|error| invalid(error.to_string()))?;
            if args.limit == Some(0) { return Err(invalid("limit must be a positive integer".into())); }
            let mut versions = repository.list_versions(id(args.task_id).map_err(|error| invalid(error.to_string()))?).map_err(store_error)?;
            versions.reverse();
            if let Some(cursor) = args.before_version_no { versions.retain(|version| version.version_no < cursor); }
            versions.truncate(args.limit.unwrap_or(100));
            Ok(json!({"versions": versions}))
        }
        "task.version.get" => {
            let args: VersionArgs = parse_args(request.args).map_err(|error| invalid(error.to_string()))?;
            Ok(json!({"version": repository.get_version(id(args.task_id).map_err(|error| invalid(error.to_string()))?, args.version_no).map_err(store_error)?}))
        }
        "task.version.compare" => {
            let args: CompareArgs = parse_args(request.args).map_err(|error| invalid(error.to_string()))?;
            Ok(serde_json::to_value(repository.compare_versions(id(args.task_id).map_err(|error| invalid(error.to_string()))?, args.from_version_no, args.to_version_no).map_err(store_error)?).unwrap_or_else(|_| json!({})))
        }
        "task.version.approve" => {
            let args: VersionArgs = parse_args(request.args).map_err(|error| invalid(error.to_string()))?;
            Ok(json!({"baseline": repository.approve_baseline(id(args.task_id).map_err(|error| invalid(error.to_string()))?, args.version_no, &actor).map_err(store_error)?}))
        }
        "task.label.add" | "task.label.remove" => {
            let args: LabelArgs = parse_args(request.args).map_err(|error| invalid(error.to_string()))?;
            if args.label.trim().is_empty() { return Err(invalid("label must not be empty".into())); }
            let task_id = id(args.task_id).map_err(|error| invalid(error.to_string()))?;
            let mut task = repository.get(task_id).map_err(store_error)?;
            let before = task.labels.clone();
            if request.operation == "task.label.add" {
                if !task.labels.iter().any(|label| label.eq_ignore_ascii_case(&args.label)) {
                    task.labels.push(args.label);
                }
            } else {
                task.labels.retain(|label| !label.eq_ignore_ascii_case(&args.label));
            }
            let changed = before != task.labels;
            let updated = repository.update(&task, &actor).map_err(store_error)?;
            Ok(json!({"task": updated, "no_op": !changed}))
        }
        "task.commit.add" => {
            let args: CommitArgs = parse_args(request.args).map_err(|error| invalid(error.to_string()))?;
            if args.revision.trim().is_empty() { return Err(invalid("revision must not be empty".into())); }
            let output = Command::new("git")
                .current_dir(root)
                .args(["rev-parse", "--verify", "--end-of-options"])
                .arg(format!("{}^{{commit}}", args.revision))
                .output()
                .map_err(|error| ApiError { code: "io_error", message: format!("Cannot run git: {error}"), retryable: false, details: json!({}) })?;
            if !output.status.success() {
                return Err(invalid(format!("Git revision '{}' does not resolve to a commit", args.revision)));
            }
            let sha = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            if !((sha.len() == 40 || sha.len() == 64) && sha.bytes().all(|byte| byte.is_ascii_hexdigit())) {
                return Err(invalid("Git returned an invalid commit object ID".into()));
            }
            let task_id = id(args.task_id).map_err(|error| invalid(error.to_string()))?;
            let mut task = repository.get(task_id).map_err(store_error)?;
            let before = task.commit_refs.clone();
            if !task.commit_refs.iter().any(|reference| reference.eq_ignore_ascii_case(&sha)) {
                task.commit_refs.push(sha.clone());
            }
            let changed = before != task.commit_refs;
            let updated = repository.update(&task, &actor).map_err(store_error)?;
            Ok(json!({"task": updated, "commit_sha": sha, "no_op": !changed}))
        }
        "task.lock.acquire" => {
            let args: LockArgs = parse_args(request.args).map_err(|error| invalid(error.to_string()))?;
            if request.lock_token.is_some() { return Err(invalid("lock_token is not accepted by task.lock.acquire".into())); }
            let task_id = id(args.task_id).map_err(|error| invalid(error.to_string()))?;
            let owner_id = new_owner_id()?;
            let grant = lock_service.acquire(task_id, &owner_id, &actor).map_err(store_error)?;
            Ok(lock_grant_json(&grant))
        }
        "task.lock.recover" => {
            let args: LockArgs = parse_args(request.args).map_err(|error| invalid(error.to_string()))?;
            if request.lock_token.is_some() { return Err(invalid("lock_token is not accepted by task.lock.recover".into())); }
            let task_id = id(args.task_id).map_err(|error| invalid(error.to_string()))?;
            let owner_id = new_owner_id()?;
            let grant = lock_service.recover(task_id, &owner_id, &actor).map_err(store_error)?;
            Ok(lock_grant_json(&grant))
        }
        "task.lock.heartbeat" | "task.lock.release" => {
            let args: LockOwnerArgs = parse_args(request.args).map_err(|error| invalid(error.to_string()))?;
            let token = request.lock_token.as_deref().ok_or_else(|| invalid("lock_token is required for heartbeat and release".into()))?;
            let task_id = id(args.task_id).map_err(|error| invalid(error.to_string()))?;
            let grant = lock_service.grant_from_token(task_id, token).map_err(store_error)?;
            if grant.owner_id != args.owner_id || grant.fencing_generation != args.fencing_generation {
                return Err(store_error(StoreError::StaleLock));
            }
            if request.operation == "task.lock.heartbeat" {
                let renewed = lock_service.heartbeat(&grant, &actor).map_err(store_error)?;
                Ok(json!({"heartbeat_at": renewed.heartbeat_at, "expires_at": renewed.expires_at}))
            } else {
                lock_service.release(&grant, &actor).map_err(store_error)?;
                Ok(json!({"released": true, "fencing_generation": grant.fencing_generation}))
            }
        }
        "store.migrate" | "store.snapshot" => {
            Err(invalid("This operation is specified but not implemented yet".into()))
        }
        other => Err(invalid(format!("Operation '{other}' is not implemented yet"))),
    }
}

fn new_owner_id() -> Result<String, ApiError> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| ApiError {
        code: "internal_error", message: format!("Cannot generate lock owner ID: {error}"),
        retryable: false, details: json!({}),
    })?;
    Ok(format!("session-{}", hex::encode(bytes)))
}

fn lock_grant_json(grant: &LockGrant) -> Value {
    json!({
        "owner_id": grant.owner_id,
        "lock_token": grant.token,
        "fencing_generation": grant.fencing_generation,
        "acquired_at": grant.acquired_at,
        "heartbeat_at": grant.heartbeat_at,
        "expires_at": grant.expires_at
    })
}

fn invalid(message: String) -> ApiError {
    ApiError { code: "invalid_request", message, retryable: false, details: json!({}) }
}

pub fn run() {
    let mut input = String::new();
    let result = io::stdin().read_to_string(&mut input)
        .map_err(|error| invalid(format!("Cannot read request: {error}")))
        .and_then(|_| serde_json::from_str::<Request>(&input).map_err(|error| invalid(format!("Invalid JSON request: {error}"))));
    let (request_id, response) = match result {
        Ok(request) => {
            let request_id = request.request_id.clone();
            let response = execute(request, Path::new("."));
            (request_id, response)
        }
        Err(error) => (None, Err(error)),
    };
    match response {
        Ok(data) => println!("{}", serde_json::to_string(&Success { api_version: 1, request_id, ok: true, data }).expect("response serialization")),
        Err(error) => {
            eprintln!("todoing api: {}", error.message);
            println!("{}", serde_json::to_string(&Failure { api_version: 1, request_id, ok: false, error }).expect("error serialization"));
            std::process::exit(2);
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup_project() -> tempfile::TempDir {
        let root = tempfile::tempdir().expect("temporary project");
        let todoing_dir = root.path().join(".todoing");
        fs::create_dir_all(&todoing_dir).expect("create project metadata");
        let config = ProjectConfig::default();
        fs::write(todoing_dir.join("config.toml"), config.to_toml().expect("serialize config"))
            .expect("write config");
        Store::initialize(root.path(), ProjectId::new("api-test-project").unwrap())
            .expect("initialize store");
        root
    }

    fn request(project_id: &str, operation: &str, args: Value) -> Request {
        Request {
            api_version: 1,
            request_id: Some("test-request".into()),
            project_id: project_id.into(),
            actor_id: "api-test-agent".into(),
            operation: operation.into(),
            args,
            lock_token: None,
        }
    }

    #[test]
    fn request_envelope_rejects_unknown_properties() {
        let parsed = serde_json::from_value::<Request>(json!({
            "api_version": 1,
            "project_id": "project",
            "actor_id": "agent",
            "operation": "task.list",
            "args": {},
            "unexpected": true
        }));
        assert!(parsed.is_err());
    }

    #[test]
    fn task_create_lock_and_owner_read_round_trip() {
        let root = setup_project();
        let created = execute(
            request("api-test-project", "task.create", json!({
                "title": "API-created task",
                "body": "created through JSON"
            })),
            root.path(),
        ).expect("create task");
        let task_id = created["task"]["id"].as_i64().expect("task ID");

        let locked = execute(
            request("api-test-project", "task.lock.acquire", json!({"task_id": task_id})),
            root.path(),
        ).expect("acquire lock");
        let token = locked["lock_token"].as_str().expect("lock token").to_owned();
        let owner_id = locked["owner_id"].as_str().expect("owner ID");
        let generation = locked["fencing_generation"].as_i64().expect("generation");

        let mut owner_read = request("api-test-project", "task.get", json!({"task_id": task_id}));
        owner_read.lock_token = Some(token.clone());
        assert!(execute(owner_read, root.path()).is_ok());

        let non_owner_read = execute(
            request("api-test-project", "task.get", json!({"task_id": task_id})),
            root.path(),
        );
        assert_eq!(non_owner_read.unwrap_err().code, "task_locked");

        let mut release = request(
            "api-test-project",
            "task.lock.release",
            json!({
                "task_id": task_id,
                "owner_id": owner_id,
                "fencing_generation": generation
            }),
        );
        release.lock_token = Some(token);
        execute(release, root.path()).expect("release lock");

        assert!(execute(
            request("api-test-project", "task.get", json!({"task_id": task_id})),
            root.path(),
        ).is_ok());
    }
}
