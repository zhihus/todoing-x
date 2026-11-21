use super::{Clock, TaskLockService};
use crate::{config::ProjectConfig, domain::{ActorContext, ActorId, ProjectId, TaskId}, storage::{Store, StoreError}};
use std::sync::{Arc, atomic::{AtomicI64, Ordering}};
use tempfile::tempdir;

#[derive(Clone)]
struct ManualClock(Arc<AtomicI64>);
impl ManualClock {
    fn new(now: i64) -> Self { Self(Arc::new(AtomicI64::new(now))) }
    fn advance(&self, millis: i64) { self.0.fetch_add(millis, Ordering::SeqCst); }
}
impl Clock for ManualClock {
    fn now_millis(&self) -> i64 { self.0.load(Ordering::SeqCst) }
}
fn actor() -> ActorContext { ActorContext::new(ActorId::new("agent:lock-tests").unwrap()) }
fn setup() -> (tempfile::TempDir, Store, TaskLockService, ManualClock, TaskId) {
    let root = tempdir().unwrap();
    let store = Store::initialize(root.path(), ProjectId::new("lock-tests").unwrap()).unwrap();
    store.connection().unwrap().execute(
        "INSERT INTO tasks(title, status_id, created_at, updated_at)
         VALUES ('locked task', 'todo', '2026-11-21T00:00:00Z', '2026-11-21T00:00:00Z')", []
    ).unwrap();
    let clock = ManualClock::new(1_000_000);
    let service = TaskLockService::with_clock(store.clone(), ProjectConfig::default(), Arc::new(clock.clone())).unwrap();
    (root, store, service, clock, TaskId::new(1).unwrap())
}

#[test]
fn acquire_returns_one_time_token_and_persists_only_its_hash() {
    let (_root, store, service, _clock, task_id) = setup();
    let grant = service.acquire(task_id, "session-a", &actor()).unwrap();
    assert_eq!(grant.task_id, task_id);
    assert_eq!(grant.owner_id, "session-a");
    assert_eq!(grant.token.len(), 64);
    assert_eq!(grant.fencing_generation, 1);
    let hash: Vec<u8> = store.connection().unwrap().query_row(
        "SELECT token_hash FROM task_locks WHERE task_id = 1", [], |row| row.get(0)
    ).unwrap();
    assert_eq!(hash.len(), 32);
    assert_ne!(hex::encode(hash), grant.token);
}

#[test]
fn active_lock_cannot_be_acquired_by_another_owner() {
    let (_root, _store, service, _clock, task_id) = setup();
    service.acquire(task_id, "session-a", &actor()).unwrap();
    assert!(matches!(service.acquire(task_id, "session-b", &actor()), Err(StoreError::TaskLocked)));
}

#[test]
fn heartbeat_renews_only_the_current_valid_owner() {
    let (_root, _store, service, clock, task_id) = setup();
    let grant = service.acquire(task_id, "session-a", &actor()).unwrap();
    let old_expiry = grant.expires_at;
    clock.advance(30_000);
    let renewed = service.heartbeat(&grant, &actor()).unwrap();
    assert_eq!(renewed.fencing_generation, grant.fencing_generation);
    assert!(renewed.expires_at > old_expiry);
    assert_eq!(renewed.heartbeat_at, clock.now_millis());
}

#[test]
fn release_retains_generation_and_next_acquisition_increments_it() {
    let (_root, _store, service, _clock, task_id) = setup();
    let first = service.acquire(task_id, "session-a", &actor()).unwrap();
    service.release(&first, &actor()).unwrap();
    let second = service.acquire(task_id, "session-b", &actor()).unwrap();
    assert_eq!(second.fencing_generation, first.fencing_generation + 1);
}

#[test]
fn recovery_requires_expiry_and_grace_and_fences_the_previous_owner() {
    let (_root, _store, service, clock, task_id) = setup();
    let first = service.acquire(task_id, "session-a", &actor()).unwrap();
    assert!(matches!(service.recover(task_id, "session-b", &actor()), Err(StoreError::TaskLocked)));
    clock.advance(300_001);
    assert!(matches!(service.recover(task_id, "session-b", &actor()), Err(StoreError::LockExpired)));
    clock.advance(60_000);
    let recovered = service.recover(task_id, "session-b", &actor()).unwrap();
    assert_eq!(recovered.fencing_generation, first.fencing_generation + 1);
    assert!(matches!(service.heartbeat(&first, &actor()), Err(StoreError::StaleLock)));
    service.heartbeat(&recovered, &actor()).unwrap();
}

#[test]
fn expired_owner_cannot_renew_during_recovery_grace() {
    let (_root, _store, service, clock, task_id) = setup();
    let grant = service.acquire(task_id, "session-a", &actor()).unwrap();
    clock.advance(300_001);
    assert!(matches!(service.heartbeat(&grant, &actor()), Err(StoreError::LockExpired)));
}

#[test]
fn lock_operations_are_recorded_with_fencing_generation() {
    let (_root, store, service, _clock, task_id) = setup();
    let grant = service.acquire(task_id, "session-a", &actor()).unwrap();
    service.heartbeat(&grant, &actor()).unwrap();
    service.release(&grant, &actor()).unwrap();
    let count: i64 = store.connection().unwrap().query_row(
        "SELECT COUNT(*) FROM operation_history
         WHERE task_id = 1 AND operation IN ('task.lock.acquire','task.lock.heartbeat','task.lock.release')
           AND fencing_generation = 1 AND outcome = 'succeeded'",
        [], |row| row.get(0)
    ).unwrap();
    assert_eq!(count, 3);
}

#[test]
fn lock_owner_id_must_be_nonempty_and_bounded() {
    let (_root, _store, service, _clock, task_id) = setup();
    assert!(service.acquire(task_id, " ", &actor()).is_err());
    assert!(service.acquire(task_id, &"x".repeat(129), &actor()).is_err());
}


#[test]
fn repository_enforces_lock_for_reads_writes_history_and_version_access() {
    use crate::storage::TaskRepository;

    let (_root, store, _service, _clock, task_id) = setup();
    store.connection().unwrap().execute(
        "INSERT INTO tasks(title, status_id, created_at, updated_at)
         VALUES ('dependency task', 'todo', '2026-11-21T00:00:00Z', '2026-11-21T00:00:00Z')",
        [],
    ).unwrap();

    let config = ProjectConfig::default();
    let repository = TaskRepository::new(store, config).unwrap();
    let actor = actor();
    let original = repository.get(task_id).unwrap();
    let service = repository.lock_service().unwrap();
    let grant = service.acquire(task_id, "session-owner", &actor).unwrap();
    let owner_repository = repository.with_lock_grant(&grant);

    assert!(matches!(repository.get(task_id), Err(StoreError::TaskLocked)));
    assert!(repository.list().unwrap().iter().all(|task| task.id != task_id));
    assert!(matches!(repository.list_versions(task_id), Err(StoreError::TaskLocked)));
    assert!(matches!(repository.get_version(task_id, 1), Err(StoreError::TaskLocked)));
    assert!(matches!(repository.get_history(task_id), Err(StoreError::TaskLocked)));
    assert!(matches!(repository.get_baseline(task_id), Err(StoreError::TaskLocked)));
    assert!(matches!(repository.update(&original, &actor), Err(StoreError::TaskLocked)));
    assert!(matches!(repository.complete(task_id, &actor), Err(StoreError::TaskLocked)));
    assert!(matches!(
        repository.add_dependency(task_id, TaskId::new(2).unwrap(), &actor),
        Err(StoreError::TaskLocked)
    ));
    assert!(matches!(repository.approve_baseline(task_id, 1, &actor), Err(StoreError::TaskLocked)));
    assert!(repository.get_store_history().unwrap().iter().all(|entry| entry.task_id != Some(task_id)));

    let mut changed = owner_repository.get(task_id).unwrap();
    changed.title = "changed by owner".into();
    assert_eq!(owner_repository.update(&changed, &actor).unwrap().title, "changed by owner");

}
