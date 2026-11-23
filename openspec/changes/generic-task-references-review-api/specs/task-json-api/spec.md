## ADDED Requirements

### Requirement: Expose generic external-reference operations

The JSON API v1 MUST expose task.external-ref.add, task.external-ref.list, and
task.external-ref.remove. Add accepts task_id, namespace, value, relation, and
optional scope (task by default, or project). Remove accepts task_id and
reference_id. Requests MUST use the standard envelope and task lock rules.
Responses MUST identify the reference and report no-op behavior.

#### Scenario: Add and list an external reference

- **WHEN** a lock owner adds a valid reference and then lists references for the task
- **THEN** the API returns the stored namespace, value, relation, and reference ID

### Requirement: Expose Review-history operations

The JSON API v1 MUST expose task.review.add and task.review.list. Add accepts
task_id, review_id, version_no, reviewer, outcome, and ordered comments. List
accepts task_id and bounded cursor/limit pagination. Review creation MUST return
the Review record, task revision_count, resulting task version, and no-op indicator.

#### Scenario: Add a Review record

- **WHEN** a lock owner adds a valid Review record for an existing task version
- **THEN** the API returns the record and updated revision_count

### Requirement: Expose final commit assignment

The JSON API v1 MUST expose task.commit.finalize. It accepts task_id and revision,
MUST resolve the supplied revision to a full SHA, and return the final commit marker.

#### Scenario: Set a final commit marker

- **WHEN** a lock owner assigns a valid final commit revision
- **THEN** the API returns the resolved SHA and resulting task version

### Requirement: Return stable extension error codes

The API MUST return the stable codes external_ref_conflict, external_ref_not_found,
review_id_conflict, and review_version_not_found for the corresponding failures.
Final-commit mutation on a terminal task MUST use the existing task_terminal error.
The response schema MUST enumerate all extension codes.

### Requirement: Include extension state in task responses

Task responses MUST include revision_count, final_commit_sha, and active external
references. Review history is retrieved through task.review.list.

#### Scenario: Read task extension fields

- **WHEN** a client reads an unlocked task
- **THEN** the response includes revision_count, final_commit_sha, and active external references
