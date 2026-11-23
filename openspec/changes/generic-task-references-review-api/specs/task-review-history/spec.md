# Task review history

## Requirements

### Requirement: Store immutable review records

The system MUST store append-only Review records linked to an existing immutable
version of the same task. Each record MUST include a stable review ID, reviewer
label, outcome, ordered comments, actor attribution, and timestamp.

#### Scenario: Add an approved review

- **WHEN** a lock owner adds an approved review for an existing task version
- **THEN** Todoing stores the record
- **AND** creates a task version and operation-history event atomically
- **AND** does not change workflow status or revision_count

### Requirement: Count requested changes exactly once

The task MUST expose a persisted revision_count. It MUST increase by exactly one
for each newly accepted Review record with outcome changes_requested. Multiple
comments in one record count as one review cycle.

#### Scenario: Record requested changes

- **WHEN** a lock owner adds a changes_requested Review record
- **THEN** revision_count increases by exactly one
- **AND** the record, task version, and history event commit atomically

### Requirement: Make review creation idempotent

The system MUST enforce uniqueness of review_id within a task.

#### Scenario: Repeat an identical review request

- **WHEN** a client repeats a review_id with an identical canonical payload
- **THEN** the original record is returned with no_op=true
- **AND** no new version or counter increment is created

#### Scenario: Reject a conflicting review request

- **WHEN** a client reuses a review_id with a different payload
- **THEN** the operation is rejected with review_id_conflict
- **AND** task state, history, and review records remain unchanged

#### Scenario: Reject a missing reviewed version

- **WHEN** a lock owner submits a Review record for a version that does not exist
- **THEN** the operation is rejected with review_version_not_found
- **AND** no Review record, task version, or counter increment is committed

### Requirement: Protect review history under locks

Review history reads and writes MUST obey task lock ownership. A non-owner MUST NOT
read review records for a foreign-locked task.

#### Scenario: Read review history under a foreign lock

- **WHEN** a client without the valid lock token requests Review history
- **THEN** the request is rejected without disclosing review data
