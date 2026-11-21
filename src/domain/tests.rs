[object Object]
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
