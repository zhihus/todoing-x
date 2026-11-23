# Task final commit reference

## Requirements

### Requirement: Set one explicit final commit marker

The system MUST allow a task to have one explicit final Git commit SHA separate
from its ordinary commit references. Todoing MUST resolve the supplied revision
to a full SHA before storing it.

#### Scenario: Set the final commit marker

- **WHEN** a lock owner sets a valid final Git revision for a task without a marker
- **THEN** Todoing stores the full SHA as the final commit marker
- **AND** creates a task version and operation-history event atomically

### Requirement: Make final commit assignment idempotent

Repeating the same final SHA MUST be a no-op. Assigning a different SHA when a
final marker already exists MUST be rejected; the marker MUST NOT be silently
replaced or cleared.

#### Scenario: Repeat the same final SHA

- **WHEN** a client sets the already assigned final SHA again
- **THEN** the current task is returned with no_op=true
- **AND** no new version is created

#### Scenario: Reject a different final SHA

- **WHEN** a client attempts to assign a different final SHA to a task that already has one
- **THEN** the operation is rejected with a stable conflict error
- **AND** the existing marker remains unchanged
