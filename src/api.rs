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
    #[serde(default)]
    archive_state: Option<String>,
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
struct ExternalReferenceAddArgs { task_id: i64, namespace: String, value: String, relation: String, #[serde(default = "default_reference_scope")] scope: String }
fn default_reference_scope() -> String { "task".into() }
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExternalReferenceListArgs { task_id: i64, #[serde(default)] include_removed: bool }
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExternalReferenceRemoveArgs { task_id: i64, reference_id: String }
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewAddArgs { task_id: i64, review_id: String, version_no: i64, reviewer: String, outcome: String, #[serde(default)] comments: Vec<Value> }
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FinalCommitArgs { task_id: i64, revision: String }

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CompareArgs { task_id: i64, from_version_no: i64, to_version_no: i64 }

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LockArgs { task_id: i64 }

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SnapshotArgs {
    output_path: String,
    #[serde(default)]
    overwrite: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MigrateArgs {
    #[serde(default)]
    dry_run: bool,
}

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

fn migration_error(error: StoreError) -> ApiError {
    let code = match &error {
        StoreError::AlreadyExists(_) | StoreError::InvalidTask(_) => "migration_conflict",
        StoreError::InvalidConfig(_) => "config_invalid",
        StoreError::Io(_) => "io_error",
        _ => return store_error(error),
    };
    ApiError {
        code,
        message: error.to_string(),
        retryable: false,
        details: json!({}),
    }
}

fn initialization_error(error: StoreError) -> ApiError {
    let (code, retryable) = match &error {
        StoreError::AlreadyExists(_) => ("store_already_initialized", false),
        StoreError::InvalidConfig(_) => ("config_invalid", false),
        StoreError::InvalidTask(_) => ("migration_required", false),
        StoreError::Io(_) => ("io_error", false),
        _ => ("internal_error", false),
    };
    ApiError { code, message: error.to_string(), retryable, details: json!({}) }
}

fn store_error(error: StoreError) -> ApiError {
    let (code, retryable) = match &error {
        StoreError::TaskNotFound(_) => ("task_not_found", false),
        StoreError::TaskTerminal => ("task_terminal", false),
        StoreError::TaskArchived => ("task_archived", false),
        StoreError::TaskLocked => ("task_locked", true),
        StoreError::LockExpired => ("lock_expired", false),
        StoreError::StaleLock => ("stale_lock", false),
        StoreError::InvalidStatusTransition(_) => ("invalid_status_transition", false),
        StoreError::DependencyNotFound(_) => ("dependency_not_found", false),
        StoreError::DependencyCycle => ("dependency_cycle", false),
        StoreError::VersionNotFound { .. } => ("version_not_found", false),
        StoreError::InvalidTask(message) if message == "external_ref_conflict" => ("external_ref_conflict", false),
        StoreError::InvalidTask(message) if message == "external_ref_not_found" => ("external_ref_not_found", false),
        StoreError::InvalidTask(message) if message == "review_id_conflict" => ("review_id_conflict", false),
        StoreError::InvalidTask(message) if message == "review_version_not_found" => ("review_version_not_found", false),
        StoreError::InvalidTask(message) if message == "final_commit_reopen_required" => ("final_commit_reopen_required", false),
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
        "store.init", "store.info", "store.verify", "store.migrate", "store.snapshot",
        "task.create", "task.list", "task.get", "task.archive", "task.restore", "task.update", "task.status.set",
        "task.complete", "task.cancel", "task.reopen", "task.label.add",
        "task.label.remove", "task.commit.add", "task.dependency.add",
        "task.dependency.remove", "task.dependency.list", "task.history.list",
        "task.version.list", "task.version.get", "task.version.compare",
        "task.version.approve", "task.lock.acquire", "task.lock.heartbeat",
        "task.lock.release", "task.lock.recover",
        "task.external_reference.add", "task.external_reference.list", "task.external_reference.remove",
        "task.review.add", "task.review.list", "task.final_commit.set",
    ];
    if !OPERATIONS.contains(&request.operation.as_str()) {
        return Err(invalid(format!("Unknown operation '{}'", request.operation)));
    }
    let project_id = ProjectId::new(request.project_id).map_err(|e| ApiError { code: "invalid_request", message: e.to_string(), retryable: false, details: json!({}) })?;
    let actor_id = ActorId::new(request.actor_id).map_err(|e| ApiError { code: "invalid_request", message: e.to_string(), retryable: false, details: json!({}) })?;
    let actor = ActorContext::new(actor_id);
    if request.operation == "store.init" {
        if request.lock_token.is_some() {
            return Err(invalid("lock_token is not accepted by store.init".into()));
        }
        parse_args::<EmptyArgs>(request.args.clone()).map_err(invalid)?;
        let report = crate::initialization::initialize(root, Some(project_id.clone()))
            .map_err(initialization_error)?;
        return Ok(json!({
            "project_id": report.project_id,
            "schema_version": report.schema_version,
            "api_version": 1
        }));
    }
    if request.operation == "store.migrate" {
        if request.lock_token.is_some() {
            return Err(invalid("lock_token is not accepted by store.migrate".into()));
        }
        let args: MigrateArgs = parse_args(request.args.clone()).map_err(invalid)?;
        let report = crate::migration::migrate(root, &project_id, actor.actor_id(), args.dry_run)
            .map_err(migration_error)?;
        return serde_json::to_value(report).map_err(|error| ApiError {
            code: "internal_error",
            message: format!("Cannot serialize migration report: {error}"),
            retryable: false,
            details: json!({}),
        });
    }
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
            let schema_version = store.verify().map_err(store_error)?.schema_version;
            Ok(json!({"project_id": store.project_id(), "schema_version": schema_version, "api_version": 1, "task_count": task_count}))
        }
        "store.verify" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Args { #[serde(default = "default_true")] deep: bool }
            fn default_true() -> bool { true }
            let args: Args = parse_args(request.args).map_err(|error| invalid(error.to_string()))?;
            let report = store.verify().map_err(store_error)?;
            Ok(json!({"integrity_check": report.integrity_check, "foreign_key_violations": report.foreign_key_violations, "schema_version": report.schema_version, "deep": args.deep}))
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
            let mut tasks = match args.archive_state.as_deref().unwrap_or("active") {
                "active" => repository.list(),
                "archived" => repository.list_archived(),
                "all" => repository.list_all(),
                _ => return Err(invalid("archive_state must be active, archived, or all".into())),
            }.map_err(store_error)?;
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
        "task.archive" => {
            let args: TaskIdArgs = parse_args(request.args).map_err(|error| invalid(error.to_string()))?;
            let result = repository.archive(id(args.task_id).map_err(|error| invalid(error.to_string()))?, &actor).map_err(store_error)?;
            Ok(json!({"task": result.task, "no_op": result.no_op}))
        }
        "task.restore" => {
            let args: TaskIdArgs = parse_args(request.args).map_err(|error| invalid(error.to_string()))?;
            let result = repository.restore(id(args.task_id).map_err(|error| invalid(error.to_string()))?, &actor).map_err(store_error)?;
            Ok(json!({"task": result.task, "no_op": result.no_op}))
        }
        "task.get" => {
            let args: TaskIdArgs = parse_args(request.args).map_err(|error| invalid(error.to_string()))?;
            let task = repository.get(id(args.task_id).map_err(|error| invalid(error.to_string()))?).map_err(store_error)?;
            let version_no = repository.list_versions(task.id).map_err(store_error)?.last().map(|version| version.version_no).unwrap_or(0);
            let baseline = repository.get_baseline(task.id).map_err(store_error)?;
            let metadata = repository.extension_metadata(task.id).map_err(store_error)?;
            Ok(json!({"task": task, "version_no": version_no, "baseline_version_no": baseline.map(|value| value.version_no), "revision_count": metadata["revision_count"], "final_commit_sha": metadata["final_commit_sha"]}))
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
            Ok(json!({"task": repository.cancel(id(args.task_id).map_err(|error| invalid(error.to_string()))?, args.reason.as_deref().filter(|reason| !reason.is_empty()), &actor).map_err(store_error)?}))
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
            let task_id = id(args.task_id).map_err(|error| invalid(error.to_string()))?;
            Ok(json!({
                "dependencies": repository.list_dependencies(task_id).map_err(store_error)?,
                "states": repository.list_dependency_states(task_id).map_err(store_error)?
            }))
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
            let comparison = repository.compare_versions(
                id(args.task_id).map_err(|error| invalid(error.to_string()))?,
                args.from_version_no,
                args.to_version_no,
            ).map_err(store_error)?;
            serde_json::to_value(comparison).map_err(|error| ApiError {
                code: "internal_error",
                message: format!("Cannot serialize version comparison: {error}"),
                retryable: false,
                details: json!({}),
            })
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
        "task.external_reference.add" => {
            let args: ExternalReferenceAddArgs = parse_args(request.args).map_err(invalid)?;
            repository.external_reference_add(id(args.task_id).map_err(invalid)?, &args.namespace, &args.value, &args.relation, &args.scope, &actor).map_err(store_error)
        }
        "task.external_reference.list" => {
            let args: ExternalReferenceListArgs = parse_args(request.args).map_err(invalid)?;
            repository.external_reference_list(id(args.task_id).map_err(invalid)?, args.include_removed).map_err(store_error)
        }
        "task.external_reference.remove" => {
            let args: ExternalReferenceRemoveArgs = parse_args(request.args).map_err(invalid)?;
            repository.external_reference_remove(id(args.task_id).map_err(invalid)?, &args.reference_id, &actor).map_err(store_error)
        }
        "task.review.add" => {
            let args: ReviewAddArgs = parse_args(request.args).map_err(invalid)?;
            repository.review_add(id(args.task_id).map_err(invalid)?, &args.review_id, args.version_no, &args.reviewer, &args.outcome, &args.comments, &actor).map_err(store_error)
        }
        "task.review.list" => {
            let args: TaskIdArgs = parse_args(request.args).map_err(invalid)?;
            repository.review_list(id(args.task_id).map_err(invalid)?).map_err(store_error)
        }
        "task.final_commit.set" => {
            let args: FinalCommitArgs = parse_args(request.args).map_err(invalid)?;
            if args.revision.trim().is_empty() { return Err(invalid("revision must not be empty".into())); }
            let output = Command::new("git").current_dir(root)
                .args(["rev-parse", "--verify", "--end-of-options"])
                .arg(format!("{}^{{commit}}", args.revision)).output()
                .map_err(|e| ApiError { code: "io_error", message: format!("Cannot run git: {e}"), retryable: false, details: json!({}) })?;
            if !output.status.success() { return Err(invalid(format!("Git revision '{}' does not resolve to a commit", args.revision))); }
            let sha = String::from_utf8_lossy(&output.stdout).trim().to_ascii_lowercase();
            repository.set_final_commit(id(args.task_id).map_err(invalid)?, &sha, &actor).map_err(store_error)
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
        "store.snapshot" => {
            let args: SnapshotArgs = parse_args(request.args).map_err(invalid)?;
            if args.output_path.trim().is_empty() {
                return Err(invalid("output_path must not be empty".into()));
            }
            let report = store.snapshot(args.output_path, args.overwrite).map_err(store_error)?;
            Ok(json!({"snapshot_path": report.path, "size_bytes": report.size_bytes, "sha256": report.sha256}))
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

fn api_exit_code(code: &str) -> i32 {
    match code {
        "invalid_request" | "unsupported_api_version" | "project_id_mismatch" | "config_invalid" | "migration_required" => 2,
        "task_not_found" | "version_not_found" | "dependency_not_found" | "external_ref_not_found" | "review_version_not_found" => 3,
        "task_terminal" | "task_archived" | "task_locked" | "lock_expired" | "stale_lock" |
        "invalid_status_transition" | "dependency_cycle" | "external_ref_conflict" | "review_id_conflict" | "final_commit_reopen_required" => 4,
        "io_error" | "store_corrupt" | "store_busy" | "history_record_failed" => 5,
        "migration_conflict" | "store_already_initialized" => 6,
        _ => 70,
    }
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
            let exit_code = api_exit_code(error.code);
            println!("{}", serde_json::to_string(&Failure { api_version: 1, request_id, ok: false, error }).expect("error serialization"));
            std::process::exit(exit_code);
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


#[cfg(test)]
mod contract_tests {
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
            request_id: Some("contract-test".into()),
            project_id: project_id.into(),
            actor_id: "contract-test-agent".into(),
            operation: operation.into(),
            args,
            lock_token: None,
        }
    }

    #[test]
    fn api_operations_cover_task_lifecycle_dependencies_versions_and_git_refs() {
        let root = setup_project();
        let info = execute(request("api-test-project", "store.info", json!({})), root.path()).unwrap();
        assert_eq!(info["task_count"], 0);
        let verified = execute(request("api-test-project", "store.verify", json!({})), root.path()).unwrap();
        assert_eq!(verified["integrity_check"], "ok");

        let first = execute(request("api-test-project", "task.create", json!({
            "title": "First API task", "body": "initial body", "labels": ["backend"]
        })), root.path()).unwrap();
        let first_id = first["task"]["id"].as_i64().unwrap();
        assert_eq!(first["version_no"], 1);
        let second = execute(request("api-test-project", "task.create", json!({
            "title": "Second API task"
        })), root.path()).unwrap();
        let second_id = second["task"]["id"].as_i64().unwrap();

        let listed = execute(request("api-test-project", "task.list", json!({
            "labels": ["BACKEND"], "limit": 10
        })), root.path()).unwrap();
        assert_eq!(listed["tasks"].as_array().unwrap().len(), 1);
        assert_eq!(listed["tasks"][0]["id"], first_id);

        let fetched = execute(request("api-test-project", "task.get", json!({"task_id": first_id})), root.path()).unwrap();
        assert_eq!(fetched["task"]["title"], "First API task");
        let updated = execute(request("api-test-project", "task.update", json!({
            "task_id": first_id, "patch": {"title": "Updated API task", "body": "updated body"}
        })), root.path()).unwrap();
        assert_eq!(updated["task"]["body"], "updated body");
        let status = execute(request("api-test-project", "task.status.set", json!({
            "task_id": first_id, "status_id": "in_progress"
        })), root.path()).unwrap();
        assert_eq!(status["task"]["status_id"], "in_progress");

        let label = execute(request("api-test-project", "task.label.add", json!({
            "task_id": first_id, "label": "urgent"
        })), root.path()).unwrap();
        assert_eq!(label["no_op"], false);
        let duplicate = execute(request("api-test-project", "task.label.add", json!({
            "task_id": first_id, "label": "URGENT"
        })), root.path()).unwrap();
        assert_eq!(duplicate["no_op"], true);
        execute(request("api-test-project", "task.label.remove", json!({
            "task_id": first_id, "label": "urgent"
        })), root.path()).unwrap();

        execute(request("api-test-project", "task.dependency.add", json!({
            "task_id": second_id, "depends_on_task_id": first_id
        })), root.path()).unwrap();
        let dependencies = execute(request("api-test-project", "task.dependency.list", json!({
            "task_id": second_id
        })), root.path()).unwrap();
        assert_eq!(dependencies["dependencies"].as_array().unwrap().len(), 1);
        let cycle = execute(request("api-test-project", "task.dependency.add", json!({
            "task_id": first_id, "depends_on_task_id": second_id
        })), root.path()).unwrap_err();
        assert_eq!(cycle.code, "dependency_cycle");
        execute(request("api-test-project", "task.dependency.remove", json!({
            "task_id": second_id, "depends_on_task_id": first_id
        })), root.path()).unwrap();

        let version_list = execute(request("api-test-project", "task.version.list", json!({
            "task_id": first_id
        })), root.path()).unwrap();
        let versions = version_list["versions"].as_array().unwrap();
        assert!(versions.len() >= 4);
        let latest_version = versions[0]["version_no"].as_i64().unwrap();
        let version = execute(request("api-test-project", "task.version.get", json!({
            "task_id": first_id, "version_no": 1
        })), root.path()).unwrap();
        assert_eq!(version["version"]["version_no"], 1);
        let comparison = execute(request("api-test-project", "task.version.compare", json!({
            "task_id": first_id, "from_version_no": 1, "to_version_no": latest_version
        })), root.path()).unwrap();
        assert!(comparison.is_object());
        let baseline = execute(request("api-test-project", "task.version.approve", json!({
            "task_id": first_id, "version_no": latest_version
        })), root.path()).unwrap();
        assert_eq!(baseline["baseline"]["version_no"], latest_version);

        let history = execute(request("api-test-project", "task.history.list", json!({
            "task_id": first_id, "limit": 100
        })), root.path()).unwrap();
        assert!(history["events"].as_array().unwrap().len() >= 5);

        std::process::Command::new("git").current_dir(root.path()).args(["init", "-q"]).status().unwrap();
        std::process::Command::new("git").current_dir(root.path()).args(["config", "user.email", "todoing-test@example.invalid"]).status().unwrap();
        std::process::Command::new("git").current_dir(root.path()).args(["config", "user.name", "Todoing Test"]).status().unwrap();
        fs::write(root.path().join("api-commit.txt"), "test commit").unwrap();
        assert!(std::process::Command::new("git").current_dir(root.path()).args(["add", "api-commit.txt"]).status().unwrap().success());
        assert!(std::process::Command::new("git").current_dir(root.path()).args(["commit", "-m", "API contract test"]).status().unwrap().success());
        let commit = execute(request("api-test-project", "task.commit.add", json!({
            "task_id": first_id, "revision": "HEAD"
        })), root.path()).unwrap();
        assert_eq!(commit["commit_sha"].as_str().unwrap().len(), 40);

        let completed = execute(request("api-test-project", "task.complete", json!({
            "task_id": first_id
        })), root.path()).unwrap();
        assert_eq!(completed["task"]["outcome"], "completed");
        let reopened = execute(request("api-test-project", "task.reopen", json!({
            "task_id": first_id
        })), root.path()).unwrap();
        assert!(reopened["task"]["outcome"].is_null());

        let cancelled = execute(request("api-test-project", "task.cancel", json!({
            "task_id": second_id, "reason": "obsolete"
        })), root.path()).unwrap();
        assert_eq!(cancelled["task"]["outcome"], "cancelled");
        let reopened = execute(request("api-test-project", "task.reopen", json!({
            "task_id": second_id
        })), root.path()).unwrap();
        assert!(reopened["task"]["outcome"].is_null());

        let mismatch = execute(request("wrong-project-id", "task.list", json!({})), root.path()).unwrap_err();
        assert_eq!(mismatch.code, "project_id_mismatch");
        let missing = execute(request("api-test-project", "task.get", json!({"task_id": 9999})), root.path()).unwrap_err();
        assert_eq!(missing.code, "task_not_found");
        let bad_args = execute(request("api-test-project", "task.get", json!({
            "task_id": first_id, "unexpected": true
        })), root.path()).unwrap_err();
        assert_eq!(bad_args.code, "invalid_request");
    }

    #[test]
    fn api_lock_heartbeat_and_owner_write_enforcement_are_consistent() {
        let root = setup_project();
        let created = execute(request("api-test-project", "task.create", json!({
            "title": "Locked task"
        })), root.path()).unwrap();
        let task_id = created["task"]["id"].as_i64().unwrap();
        let locked = execute(request("api-test-project", "task.lock.acquire", json!({
            "task_id": task_id
        })), root.path()).unwrap();
        let token = locked["lock_token"].as_str().unwrap().to_owned();
        let owner_id = locked["owner_id"].as_str().unwrap().to_owned();
        let generation = locked["fencing_generation"].as_i64().unwrap();

        let denied = execute(request("api-test-project", "task.update", json!({
            "task_id": task_id, "patch": {"title": "Unauthorized"}
        })), root.path()).unwrap_err();
        assert_eq!(denied.code, "task_locked");

        let mut heartbeat = request("api-test-project", "task.lock.heartbeat", json!({
            "task_id": task_id, "owner_id": owner_id, "fencing_generation": generation
        }));
        heartbeat.lock_token = Some(token.clone());
        let renewed = execute(heartbeat, root.path()).unwrap();
        assert!(renewed["expires_at"].as_i64().unwrap() > renewed["heartbeat_at"].as_i64().unwrap());

        let mut owner_update = request("api-test-project", "task.update", json!({
            "task_id": task_id, "patch": {"title": "Authorized"}
        }));
        owner_update.lock_token = Some(token.clone());
        let updated = execute(owner_update, root.path()).unwrap();
        assert_eq!(updated["task"]["title"], "Authorized");

        let mut release = request("api-test-project", "task.lock.release", json!({
            "task_id": task_id, "owner_id": owner_id, "fencing_generation": generation
        }));
        release.lock_token = Some(token);
        execute(release, root.path()).unwrap();
        let final_task = execute(request("api-test-project", "task.get", json!({
            "task_id": task_id
        })), root.path()).unwrap();
        assert_eq!(final_task["task"]["title"], "Authorized");
    }

    #[test]
    fn api_exit_codes_are_stable_for_documented_error_classes() {
        assert_eq!(api_exit_code("invalid_request"), 2);
        assert_eq!(api_exit_code("task_not_found"), 3);
        assert_eq!(api_exit_code("task_locked"), 4);
        assert_eq!(api_exit_code("store_busy"), 5);
        assert_eq!(api_exit_code("migration_conflict"), 6);
        assert_eq!(api_exit_code("internal_error"), 70);
    }


    #[test]
    fn api_archive_restore_and_archive_state_filtering_are_consistent() {
        let root = setup_project();
        let first = execute(request("api-test-project", "task.create", json!({
            "title": "Archive candidate"
        })), root.path()).unwrap();
        let first_id = first["task"]["id"].as_i64().unwrap();
        let second = execute(request("api-test-project", "task.create", json!({
            "title": "Active candidate"
        })), root.path()).unwrap();
        let second_id = second["task"]["id"].as_i64().unwrap();
        execute(request("api-test-project", "task.dependency.add", json!({
            "task_id": second_id, "depends_on_task_id": first_id
        })), root.path()).unwrap();

        let archived = execute(request("api-test-project", "task.archive", json!({
            "task_id": first_id
        })), root.path()).unwrap();
        assert_eq!(archived["no_op"], false);
        assert!(archived["task"]["archived_at"].is_string());
        assert_eq!(archived["task"]["archived_by"], "contract-test-agent");

        let active = execute(request("api-test-project", "task.list", json!({})), root.path()).unwrap();
        assert_eq!(active["tasks"].as_array().unwrap().len(), 1);
        assert_eq!(active["tasks"][0]["id"], second_id);

        let archive_view = execute(request("api-test-project", "task.list", json!({
            "archive_state": "archived"
        })), root.path()).unwrap();
        assert_eq!(archive_view["tasks"].as_array().unwrap().len(), 1);
        assert_eq!(archive_view["tasks"][0]["id"], first_id);

        let all = execute(request("api-test-project", "task.list", json!({
            "archive_state": "all"
        })), root.path()).unwrap();
        assert_eq!(all["tasks"].as_array().unwrap().len(), 2);

        let dependency_state = execute(request("api-test-project", "task.dependency.list", json!({
            "task_id": second_id
        })), root.path()).unwrap();
        assert_eq!(dependency_state["dependencies"][0], first_id);
        assert_eq!(dependency_state["states"][0]["archived"], true);
        assert_eq!(dependency_state["states"][0]["locked"], false);

        let read = execute(request("api-test-project", "task.get", json!({
            "task_id": first_id
        })), root.path()).unwrap();
        assert_eq!(read["task"]["archived_at"], archived["task"]["archived_at"]);

        let denied = execute(request("api-test-project", "task.update", json!({
            "task_id": first_id, "patch": {"title": "cannot edit"}
        })), root.path()).unwrap_err();
        assert_eq!(denied.code, "task_archived");

        let lock_denied = execute(request("api-test-project", "task.lock.acquire", json!({
            "task_id": first_id
        })), root.path()).unwrap_err();
        assert_eq!(lock_denied.code, "task_archived");

        let repeated = execute(request("api-test-project", "task.archive", json!({
            "task_id": first_id
        })), root.path()).unwrap();
        assert_eq!(repeated["no_op"], true);

        let restored = execute(request("api-test-project", "task.restore", json!({
            "task_id": first_id
        })), root.path()).unwrap();
        assert_eq!(restored["no_op"], false);
        assert!(restored["task"]["archived_at"].is_null());
        assert!(restored["task"]["archived_by"].is_null());

        let repeated_restore = execute(request("api-test-project", "task.restore", json!({
            "task_id": first_id
        })), root.path()).unwrap();
        assert_eq!(repeated_restore["no_op"], true);

        let final_list = execute(request("api-test-project", "task.list", json!({})), root.path()).unwrap();
        assert_eq!(final_list["tasks"].as_array().unwrap().len(), 2);
    }

}
