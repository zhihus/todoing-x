# Task identity and lifecycle

## ADDED Requirements

### Requirement: Stable task identifiers

Todoing MUST assign each task a stable identifier that is never reused, including after a task is archived or otherwise removed from the active task view.

#### Scenario: Preserve identifier uniqueness

- **WHEN** a task leaves the active task set
- **THEN** its identifier remains reserved
- **AND** future tasks receive different identifiers

### Requirement: Prohibit physical task deletion

Todoing MUST NOT expose an operation that physically deletes a task record from its managed task history or storage.

#### Scenario: Retain a task after it leaves active work

- **WHEN** a task is no longer part of active work
- **THEN** its task record and historical data remain recoverable
- **AND** its identifier is not made available for reuse

Archiving and restoration are separate post-MVP capabilities. They are not defined or implemented by this change.
