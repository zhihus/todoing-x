use crate::{
    config::ProjectConfig,
    domain::ProjectId,
    storage::{Store, StoreError},
    TODOING_DIR,
};
use std::{
    fs,
    io::Write,
    path::Path,
};
use tempfile::NamedTempFile;

#[derive(Debug)]
pub struct InitializationReport {
    pub project_id: String,
    pub schema_version: u32,
}

pub fn initialize(
    root: &Path,
    requested_project_id: Option<ProjectId>,
) -> Result<InitializationReport, StoreError> {
    let todoing_dir = root.join(TODOING_DIR);
    fs::create_dir_all(&todoing_dir)?;

    let database_path = todoing_dir.join("todoing.db");
    if database_path.exists() {
        return Err(StoreError::AlreadyExists(database_path));
    }

    reject_legacy_store(root)?;
    ensure_project_config(&todoing_dir)?;

    let project_id = match requested_project_id {
        Some(project_id) => project_id,
        None => generate_project_id()?,
    };

    let store = Store::initialize(root, project_id)?;
    let integrity = store.verify()?;
    Ok(InitializationReport {
        project_id: store.project_id().as_str().to_owned(),
        schema_version: integrity.schema_version,
    })
}

fn reject_legacy_store(root: &Path) -> Result<(), StoreError> {
    let todoing_dir = root.join(TODOING_DIR);
    let tasks_dir = todoing_dir.join("tasks");
    if tasks_dir.is_dir() {
        for entry in fs::read_dir(&tasks_dir)? {
            let path = entry?.path();
            if path.extension().and_then(|value| value.to_str()) == Some("md") {
                return Err(StoreError::InvalidTask(
                    "Legacy Markdown tasks exist; use 'todoing migrate --project-id <id>' instead of init".into(),
                ));
            }
        }
    }
    let legacy_index = todoing_dir.join("index.json");
    if legacy_index.exists() {
        return Err(StoreError::InvalidTask(
            "Legacy index.json exists; use 'todoing migrate --project-id <id>' instead of init".into(),
        ));
    }
    Ok(())
}

fn ensure_project_config(todoing_dir: &Path) -> Result<(), StoreError> {
    let config_path = todoing_dir.join("config.toml");
    match fs::read_to_string(&config_path) {
        Ok(source) => {
            ProjectConfig::from_toml(&source)
                .map_err(|error| StoreError::InvalidConfig(error.to_string()))?;
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let source = ProjectConfig::default()
                .to_toml()
                .map_err(|error| StoreError::InvalidConfig(error.to_string()))?;
            let mut temporary = NamedTempFile::new_in(todoing_dir)?;
            temporary.write_all(source.as_bytes())?;
            temporary.as_file().sync_all()?;
            match temporary.persist_noclobber(&config_path) {
                Ok(_) => Ok(()),
                Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                    let source = fs::read_to_string(&config_path)?;
                    ProjectConfig::from_toml(&source)
                        .map_err(|error| StoreError::InvalidConfig(error.to_string()))?;
                    Ok(())
                }
                Err(error) => Err(StoreError::Io(error.error)),
            }
        }
        Err(error) => Err(StoreError::Io(error)),
    }
}

fn generate_project_id() -> Result<ProjectId, StoreError> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|error| StoreError::Io(std::io::Error::other(format!("Cannot generate project ID: {error}"))))?;
    ProjectId::new(format!("p-{}", hex::encode(bytes)))
        .map_err(|error| StoreError::InvalidTask(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn initializes_new_store_and_generates_project_id_when_omitted() {
        let root = tempdir().unwrap();
        let report = initialize(root.path(), None).unwrap();
        assert!(report.project_id.starts_with("p-"));
        assert_eq!(report.project_id.len(), 34);
        assert_eq!(report.schema_version, 2);
        let store = Store::open(root.path(), Some(&ProjectId::new(report.project_id).unwrap())).unwrap();
        assert_eq!(store.verify().unwrap().integrity_check, "ok");
    }

    #[test]
    fn preserves_valid_existing_configuration_byte_for_byte() {
        let root = tempdir().unwrap();
        let config_dir = root.path().join(TODOING_DIR);
        fs::create_dir_all(&config_dir).unwrap();
        let config_path = config_dir.join("config.toml");
        let source = ProjectConfig::default().to_toml().unwrap();
        fs::write(&config_path, &source).unwrap();

        initialize(root.path(), Some(ProjectId::new("test-project").unwrap())).unwrap();

        assert_eq!(fs::read_to_string(config_path).unwrap(), source);
    }

    #[test]
    fn invalid_configuration_prevents_database_creation() {
        let root = tempdir().unwrap();
        let config_dir = root.path().join(TODOING_DIR);
        fs::create_dir_all(&config_dir).unwrap();
        fs::write(config_dir.join("config.toml"), "not = [valid").unwrap();

        assert!(matches!(
            initialize(root.path(), Some(ProjectId::new("test-project").unwrap())),
            Err(StoreError::InvalidConfig(_))
        ));
        assert!(!config_dir.join("todoing.db").exists());
    }

    #[test]
    fn existing_database_is_never_overwritten() {
        let root = tempdir().unwrap();
        let id = ProjectId::new("test-project").unwrap();
        Store::initialize(root.path(), id.clone()).unwrap();
        assert!(matches!(initialize(root.path(), Some(id)), Err(StoreError::AlreadyExists(_))));
    }

    #[test]
    fn concurrent_initializers_activate_exactly_one_project_store() {
        let root = tempdir().unwrap();
        let root_a = root.path().to_path_buf();
        let root_b = root.path().to_path_buf();

        let first = std::thread::spawn(move || {
            initialize(&root_a, Some(ProjectId::new("project-a").unwrap()))
                .map(|report| report.project_id)
                .map_err(|error| error.to_string())
        });
        let second = std::thread::spawn(move || {
            initialize(&root_b, Some(ProjectId::new("project-b").unwrap()))
                .map(|report| report.project_id)
                .map_err(|error| error.to_string())
        });

        let results = [first.join().unwrap(), second.join().unwrap()];
        let successes: Vec<_> = results.iter().filter_map(|result| result.as_ref().ok()).collect();
        let failures: Vec<_> = results.iter().filter_map(|result| result.as_ref().err()).collect();
        assert_eq!(successes.len(), 1);
        assert_eq!(failures.len(), 1);
        assert!(failures[0].contains("already exists"));

        let actual = Store::open(root.path(), None).unwrap();
        assert_eq!(actual.project_id().as_str(), successes[0].as_str());
    }

    #[test]
    fn legacy_markdown_tasks_require_migration() {
        let root = tempdir().unwrap();
        let tasks = root.path().join(TODOING_DIR).join("tasks");
        fs::create_dir_all(&tasks).unwrap();
        fs::write(tasks.join("1.md"), "legacy task").unwrap();

        assert!(matches!(
            initialize(root.path(), Some(ProjectId::new("test-project").unwrap())),
            Err(StoreError::InvalidTask(_))
        ));
        assert!(!root.path().join(TODOING_DIR).join("todoing.db").exists());
    }
}
