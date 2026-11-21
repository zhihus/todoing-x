mod error;
mod lock;
mod store;
mod task_repository;

#[cfg(test)]
mod task_repository_tests;
#[cfg(test)]
mod task_history_tests;
#[cfg(test)]
mod status_workflow_tests;
#[cfg(test)]
mod dependency_tests;
#[cfg(test)]
mod lock_tests;
#[cfg(test)]
mod tests;

pub use error::StoreError;
pub use lock::{Clock, LockGrant, SystemClock, TaskLockService};
pub use store::{IntegrityReport, SnapshotReport, Store};
pub use task_repository::{
    HistoryEntry, HistoryOutcome, NewTask, Task, TaskBaseline, TaskFieldChange,
    TaskRepository, TaskVersion, TaskVersionDiff,
};
