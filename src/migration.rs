use crate::{
    config::{LockConfig, ProjectConfig, StatusDefinition, StatusTransition},
    domain::{ActorContext, ActorId, ProjectId, StatusId, TaskOutcome},
    storage::{Store as SqliteStore, StoreError, TaskRepository},
    Task as LegacyTask,
    TaskDateTime,
    TaskStatus as LegacyStatus,
    Store as LegacyStore,
    TODOING_DIR,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use tempfile::{Builder, NamedTempFile};

#[derive(Clone, Debug, Serialize)]
pub struct SourceHash {
    pub path: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct MigrationReport {
    pub project_id: String,
    pub task_count: usize,
    pub warnings: Vec<String>,
    pub source_hashes: Vec<SourceHash>,
    pub dry_run: bool,
    pub activated: bool,
    pub report_path: Option<PathBuf>,
}

pub fn migrate(
    root: &Path,
    project_id: &ProjectId,
    actor_id: &ActorId,
    dry_run: bool,
) -> Result<MigrationReport, StoreError> {
    let todoing_dir = root.join(TODOING_DIR);
    let database_path = todoing_dir.join("todoing.db");
    if database_path.exists() {
        return Err(StoreError::AlreadyExists(database_path));
    }

    let legacy = LegacyStore::new(root);
    let (tasks, mut warnings) = legacy.read_tasks_for_migration()
        .map_err(|message| StoreError::InvalidTask(format!("Cannot read legacy task files: {message}")))?;
    let source_hashes = collect_source_hashes(root, &mut warnings)?;

    let config_path = todoing_dir.join("config.toml");
    let (config, generated_config) = if config_path.exists() {
        let source = fs::read_to_string(&config_path)?;
        (ProjectConfig::from_toml(&source)
            .map_err(|error| StoreError::InvalidConfig(error.to_string()))?, None)
    } else {
        let config = legacy_compatibility_config();
        (config.clone(), Some(config.to_toml()
            .map_err(|error| StoreError::InvalidConfig(error.to_string()))?))
    };
    validate_legacy_config(&config)?;

    let mut seen_ids = HashSet::new();
    for task in &tasks {
        validate_legacy_task(task)?;
        if !seen_ids.insert(task.id) {
            return Err(StoreError::InvalidTask(format!("Duplicate legacy task ID: {}", task.id)));
        }
        if matches!(&task.added_at, TaskDateTime::Naive(_)) {
            warnings.push(format!(
                "Task #{} has a timezone-naive added_at value; the original timestamp was preserved without assigning a timezone",
                task.id
            ));
        }
    }

    if dry_run {
        return Ok(MigrationReport {
            project_id: project_id.as_str().to_owned(),
            task_count: tasks.len(),
            warnings,
            source_hashes,
            dry_run: true,
            activated: false,
            report_path: None,
        });
    }

    fs::create_dir_all(todoing_dir.join("migration"))?;
    let staging_root = Builder::new()
        .prefix("staging-")
        .tempdir_in(todoing_dir.join("migration"))?;
    let staging_store = SqliteStore::initialize(staging_root.path(), project_id.clone())?;
    let repository = TaskRepository::new(staging_store.clone(), config.clone())?;
    let actor = ActorContext::new(actor_id.clone());

    for task in &tasks {
        let (status_id, outcome) = legacy_status(task.status);
        repository.import_legacy_task(
            task.id,
            task.title.clone(),
            StatusId::new(status_id).map_err(|error| StoreError::InvalidTask(error.to_string()))?,
            outcome,
            task.body.clone(),
            task.added_at.isoformat(),
            task.labels.clone(),
            task.commits.clone(),
            &actor,
        )?;
    }
    staging_store.verify()?;
    drop(repository);
    drop(staging_store);

    if let Some(config_text) = generated_config {
        write_new_file(&config_path, config_text.as_bytes())?;
    }

    // hard_link provides an atomic, no-clobber activation on the same volume.
    // Removing the staging name after linking leaves one active database file.
    let staging_database = staging_root.path().join(TODOING_DIR).join("todoing.db");
    fs::hard_link(&staging_database, &database_path)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                StoreError::AlreadyExists(database_path.clone())
            } else {
                StoreError::Io(error)
            }
        })?;
    if let Err(error) = fs::remove_file(&staging_database) {
        warnings.push(format!("Activated database, but could not remove staging link: {error}"));
    }

    let active_store = SqliteStore::open(root, Some(project_id))?;
    active_store.verify()?;
    drop(active_store);

    let mut report = MigrationReport {
        project_id: project_id.as_str().to_owned(),
        task_count: tasks.len(),
        warnings,
        source_hashes,
        dry_run: false,
        activated: true,
        report_path: None,
    };
    match write_report(&todoing_dir.join("migration"), &report) {
        Ok(path) => report.report_path = Some(path),
        Err(error) => report.warnings.push(format!("Migration completed, but report file could not be written: {error}")),
    }
    Ok(report)
}

fn legacy_compatibility_config() -> ProjectConfig {
    let statuses = vec![
        StatusDefinition::new("todo", "To Do"),
        StatusDefinition::new("in_progress", "In Progress"),
        StatusDefinition::with_outcome("done", "Done", TaskOutcome::Completed),
        StatusDefinition::with_outcome("cancelled", "Cancelled", TaskOutcome::Cancelled),
    ];
    let ids = ["todo", "in_progress", "done", "cancelled"];
    let mut transitions = Vec::new();
    for from in ids {
        for to in ids {
            if from != to {
                transitions.push(StatusTransition::new(from, to));
            }
        }
    }
    ProjectConfig {
        format_version: 1,
        locking: LockConfig::default(),
        statuses,
        transitions,
    }
}

fn validate_legacy_config(config: &ProjectConfig) -> Result<(), StoreError> {
    let expected = [
        ("todo", None),
        ("in_progress", None),
        ("done", Some(TaskOutcome::Completed)),
        ("cancelled", Some(TaskOutcome::Cancelled)),
    ];
    for (id, outcome) in expected {
        let Some(status) = config.statuses.iter().find(|status| status.id == id) else {
            return Err(StoreError::InvalidConfig(format!(
                "Migration requires legacy status '{id}' in config.toml"
            )));
        };
        if status.outcome != outcome {
            return Err(StoreError::InvalidConfig(format!(
                "Legacy status '{id}' must map to outcome {outcome:?}"
            )));
        }
    }
    Ok(())
}

fn legacy_status(status: LegacyStatus) -> (&'static str, Option<TaskOutcome>) {
    match status {
        LegacyStatus::Todo => ("todo", None),
        LegacyStatus::InProgress => ("in_progress", None),
        LegacyStatus::Done => ("done", Some(TaskOutcome::Completed)),
        LegacyStatus::Cancelled => ("cancelled", Some(TaskOutcome::Cancelled)),
    }
}

fn validate_legacy_task(task: &LegacyTask) -> Result<(), StoreError> {
    if task.id <= 0 {
        return Err(StoreError::InvalidTask(format!("Legacy task ID must be positive: {}", task.id)));
    }
    task.validate_for_creation()
        .and_then(|_| task.validate())
        .map_err(StoreError::InvalidTask)?;
    Ok(())
}

fn collect_source_hashes(root: &Path, warnings: &mut Vec<String>) -> Result<Vec<SourceHash>, StoreError> {
    let todoing_dir = root.join(TODOING_DIR);
    let tasks_dir = todoing_dir.join("tasks");
    let mut paths = Vec::new();
    if tasks_dir.exists() {
        for entry in fs::read_dir(&tasks_dir)? {
            let path = entry?.path();
            if path.extension().and_then(|value| value.to_str()) == Some("md") {
                paths.push(path);
            }
        }
    }
    let index_path = todoing_dir.join("index.json");
    if index_path.exists() {
        let bytes = fs::read(&index_path)?;
        if serde_json::from_slice::<serde_json::Value>(&bytes).is_err() {
            warnings.push("Legacy index.json is malformed and was ignored; Markdown files remain the migration source of truth".into());
        } else {
            warnings.push("Legacy index.json was ignored; Markdown files are the migration source of truth".into());
        }
        paths.push(index_path);
    }
    paths.sort();
    let mut hashes = Vec::with_capacity(paths.len());
    for path in paths {
        let bytes = fs::read(&path)?;
        let relative = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
        hashes.push(SourceHash {
            path: relative,
            sha256: hex::encode(Sha256::digest(&bytes)),
        });
    }
    Ok(hashes)
}

fn write_new_file(path: &Path, contents: &[u8]) -> Result<(), StoreError> {
    let parent = path.parent().ok_or_else(|| StoreError::InvalidTask("Invalid destination path".into()))?;
    fs::create_dir_all(parent)?;
    let mut temporary = NamedTempFile::new_in(parent)?;
    temporary.write_all(contents)?;
    temporary.as_file().sync_all()?;
    temporary.persist_noclobber(path).map_err(|error| {
        if error.error.kind() == std::io::ErrorKind::AlreadyExists {
            StoreError::AlreadyExists(path.to_path_buf())
        } else {
            StoreError::Io(error.error)
        }
    })?;
    Ok(())
}

fn write_report(directory: &Path, report: &MigrationReport) -> Result<PathBuf, StoreError> {
    fs::create_dir_all(directory)?;
    let mut nonce = [0u8; 8];
    getrandom::fill(&mut nonce).map_err(|error| {
        StoreError::Io(std::io::Error::other(format!("Cannot generate migration report name: {error}")))
    })?;
    let path = directory.join(format!("migration-report-{}.json", hex::encode(nonce)));
    let mut saved_report = report.clone();
    saved_report.report_path = Some(path.clone());
    let bytes = serde_json::to_vec_pretty(&saved_report)
        .map_err(|error| StoreError::InvalidTask(error.to_string()))?;
    let mut temporary = NamedTempFile::new_in(directory)?;
    temporary.write_all(&bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist_noclobber(&path).map_err(|error| StoreError::Io(error.error))?;
    Ok(path)
}
