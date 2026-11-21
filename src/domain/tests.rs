use super::{ActorId, AppError, ErrorCode, ProjectId, StatusId, TaskId, TaskOutcome};

#[test]
fn task_ids_must_be_positive() {
    assert_eq!(TaskId::new(42).unwrap().get(), 42);
    assert!(TaskId::new(0).is_err());
    assert!(TaskId::new(-1).is_err());
}

#[test]
fn status_ids_are_stable_slugs() {
    assert_eq!(StatusId::new("in_review").unwrap().as_str(), "in_review");
    assert_eq!(StatusId::new("ready-2").unwrap().as_str(), "ready-2");
    for invalid in ["", "In Review", "UPPER", "-leading", "bad/value", "white space"] {
        assert!(StatusId::new(invalid).is_err(), "accepted invalid status ID: {invalid}");
    }
}

#[test]
fn actor_ids_are_non_empty_and_bounded() {
    assert_eq!(ActorId::new("agent:executor-1").unwrap().as_str(), "agent:executor-1");
    assert!(ActorId::new("").is_err());
    assert!(ActorId::new("   ").is_err());
    assert!(ActorId::new("x".repeat(129)).is_err());
}

#[test]
fn outcomes_have_stable_snake_case_serialization() {
    assert_eq!(serde_json::to_string(&TaskOutcome::Completed).unwrap(), "\"completed\"");
    assert_eq!(serde_json::to_string(&TaskOutcome::Cancelled).unwrap(), "\"cancelled\"");
    assert_eq!(serde_json::from_str::<TaskOutcome>("\"cancelled\"").unwrap(), TaskOutcome::Cancelled);
}

#[test]
fn error_codes_have_stable_strings_and_exit_codes() {
    assert_eq!(ErrorCode::TaskLocked.as_str(), "task_locked");
    assert_eq!(ErrorCode::TaskTerminal.as_str(), "task_terminal");
    assert_eq!(ErrorCode::TaskLocked.exit_code(), 4);
    assert_eq!(ErrorCode::StoreBusy.exit_code(), 5);
    assert_eq!(ErrorCode::MigrationConflict.exit_code(), 6);
    assert_eq!(ErrorCode::InternalError.exit_code(), 70);
    assert!(ErrorCode::TaskLocked.is_retryable());
    assert!(!ErrorCode::TaskTerminal.is_retryable());
}

#[test]
fn application_error_preserves_machine_code_and_human_message() {
    let error = AppError::new(ErrorCode::DependencyCycle, "dependency would create a cycle");
    assert_eq!(error.code(), ErrorCode::DependencyCycle);
    assert_eq!(error.message(), "dependency would create a cycle");
    assert_eq!(error.exit_code(), 4);
    assert_eq!(error.to_string(), "dependency would create a cycle");
}

#[test]
fn task_ids_serialize_as_positive_json_integers() {
    let id = TaskId::new(7).unwrap();
    assert_eq!(serde_json::to_string(&id).unwrap(), "7");
    assert_eq!(serde_json::from_str::<TaskId>("7").unwrap(), id);
    assert!(serde_json::from_str::<TaskId>("0").is_err());
    assert!(serde_json::from_str::<TaskId>("\"7\"").is_err());
}

#[test]
fn status_and_actor_ids_validate_during_json_deserialization() {
    assert_eq!(serde_json::from_str::<StatusId>("\"in_review\"").unwrap().as_str(), "in_review");
    assert!(serde_json::from_str::<StatusId>("\"Bad Status\"").is_err());
    assert_eq!(serde_json::from_str::<ActorId>("\"agent:worker\"").unwrap().as_str(), "agent:worker");
    assert!(serde_json::from_str::<ActorId>("\" agent:worker\"").is_err());
}

#[test]
fn project_ids_are_non_empty_bounded_and_json_safe() {
    let id = ProjectId::new("investplan").unwrap();
    assert_eq!(id.as_str(), "investplan");
    assert_eq!(serde_json::to_string(&id).unwrap(), "\"investplan\"");
    assert_eq!(serde_json::from_str::<ProjectId>("\"investplan\"").unwrap(), id);
    assert!(ProjectId::new(" ").is_err());
    assert!(ProjectId::new(" project ").is_err());
    assert!(ProjectId::new("x".repeat(129)).is_err());
}
