mod error;
mod store;
mod task_repository;

#[cfg(test)]
mod task_repository_tests;
#[cfg(test)]
mod task_history_tests;
#[cfg(test)]
mod tests;

pub use error::StoreError;
pub use store::{IntegrityReport, Store};
pub use task_repository::{
    HistoryEntry, HistoryOutcome, NewTask, Task, TaskBaseline, TaskFieldChange,
    TaskRepository, TaskVersion, TaskVersionDiff,
};
