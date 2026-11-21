use super::{NewTask, Store, StoreError, TaskRepository};
use crate::{
    config::ProjectConfig,
    domain::{ActorContext, ActorId, ProjectId, StatusId},
};
use tempfile::tempdir;

fn actor() -> ActorContext {
    ActorContext::new(ActorId::new("agent:dependency-tests").unwrap())
}

fn repository() -> (tempfile::TempDir, TaskRepository) {
    let root = tempdir().unwrap();
    let store = Store::initialize(root.path(), ProjectId::new("dependency-tests").unwrap()).unwrap();
    (root, TaskRepository::new(store, ProjectConfig::default()).unwrap())
}

fn new_task(title: &str) -> NewTask {
    NewTask {
        title: title.into(),
        status_id: StatusId::new("todo").unwrap(),
        outcome: None,
        body: String::new(),
        labels: vec![],
        commit_refs: vec![],
    }
}

#[test]
fn adding_dependency_updates_task_version_history_and_snapshot() {
    let (_root, repository) = repository();
    let dependent = repository.create(new_task("dependent"), &actor()).unwrap();
    let prerequisite = repository.create(new_task("prerequisite"), &actor()).unwrap();

    repository.add_dependency(dependent.id, prerequisite.id, &actor()).unwrap();

    let updated = repository.get(dependent.id).unwrap();
    assert_eq!(updated.dependencies, vec![prerequisite.id]);
    assert_eq!(repository.list_dependencies(dependent.id).unwrap(), vec![prerequisite.id]);
    let versions = repository.list_versions(dependent.id).unwrap();
    assert_eq!(versions.len(), 2);
    assert!(versions[1].snapshot_json.contains(&format!("[{}]", prerequisite.id.get())));
    let history = repository.get_history(dependent.id).unwrap();
    assert_eq!(history.last().unwrap().operation, "task.dependency.add");
}

#[test]
fn removing_dependency_updates_snapshot_and_history() {
    let (_root, repository) = repository();
    let dependent = repository.create(new_task("dependent"), &actor()).unwrap();
    let prerequisite = repository.create(new_task("prerequisite"), &actor()).unwrap();
    repository.add_dependency(dependent.id, prerequisite.id, &actor()).unwrap();

    repository.remove_dependency(dependent.id, prerequisite.id, &actor()).unwrap();

    assert!(repository.get(dependent.id).unwrap().dependencies.is_empty());
    assert_eq!(repository.list_versions(dependent.id).unwrap().len(), 3);
    assert_eq!(
        repository.get_history(dependent.id).unwrap().last().unwrap().operation,
        "task.dependency.remove"
    );
}

#[test]
fn duplicate_dependency_add_is_a_no_op() {
    let (_root, repository) = repository();
    let dependent = repository.create(new_task("dependent"), &actor()).unwrap();
    let prerequisite = repository.create(new_task("prerequisite"), &actor()).unwrap();
    repository.add_dependency(dependent.id, prerequisite.id, &actor()).unwrap();

    repository.add_dependency(dependent.id, prerequisite.id, &actor()).unwrap();

    assert_eq!(repository.list_versions(dependent.id).unwrap().len(), 2);
    assert_eq!(repository.get_history(dependent.id).unwrap().len(), 2);
}

#[test]
fn self_dependency_is_rejected_and_recorded() {
    let (_root, repository) = repository();
    let task = repository.create(new_task("self"), &actor()).unwrap();

    assert!(matches!(
        repository.add_dependency(task.id, task.id, &actor()),
        Err(StoreError::DependencyCycle)
    ));

    assert!(repository.get(task.id).unwrap().dependencies.is_empty());
    assert_eq!(
        repository.get_history(task.id).unwrap().last().unwrap().outcome,
        super::HistoryOutcome::Rejected
    );
}

#[test]
fn indirect_dependency_cycle_is_rejected() {
    let (_root, repository) = repository();
    let a = repository.create(new_task("a"), &actor()).unwrap();
    let b = repository.create(new_task("b"), &actor()).unwrap();
    let c = repository.create(new_task("c"), &actor()).unwrap();

    repository.add_dependency(a.id, b.id, &actor()).unwrap();
    repository.add_dependency(b.id, c.id, &actor()).unwrap();

    assert!(matches!(
        repository.add_dependency(c.id, a.id, &actor()),
        Err(StoreError::DependencyCycle)
    ));

    assert!(repository.get(c.id).unwrap().dependencies.is_empty());
}

#[test]
fn missing_dependency_task_is_rejected_and_recorded() {
    let (_root, repository) = repository();
    let task = repository.create(new_task("dependent"), &actor()).unwrap();
    let missing = crate::domain::TaskId::new(999).unwrap();

    assert!(matches!(
        repository.add_dependency(task.id, missing, &actor()),
        Err(StoreError::DependencyNotFound(_))
    ));

    assert_eq!(repository.get_history(task.id).unwrap().last().unwrap().outcome, super::HistoryOutcome::Rejected);
}

#[test]
fn task_update_cannot_bypass_dependency_operations() {
    let (_root, repository) = repository();
    let dependent = repository.create(new_task("dependent"), &actor()).unwrap();
    let prerequisite = repository.create(new_task("prerequisite"), &actor()).unwrap();
    let mut changed = dependent.clone();
    changed.dependencies.push(prerequisite.id);

    assert!(repository.update(&changed, &actor()).is_err());
    assert!(repository.get(dependent.id).unwrap().dependencies.is_empty());
}
