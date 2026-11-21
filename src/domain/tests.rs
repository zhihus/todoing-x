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
