# Git commit references

## Purpose

Describe how the current Todoing implementation stores Git commit references on tasks.

## Requirements

### Requirement: Store resolved commit references

The system MUST allow a task to be associated with one or more Git commit references.

When a revision is supplied through the CLI, the system MUST resolve it to a full commit SHA before storing it.

#### Scenario: Add a commit reference

- **WHEN** the user links a valid revision to an existing task
- **THEN** the task stores the resolved full SHA
- **AND** the reference is available in task inspection and JSON output

### Requirement: Preserve commit references during Rust task updates

The Rust implementation MUST preserve a task's commit references when supported task fields are updated.

#### Scenario: Update a linked task

- **WHEN** the user changes a linked task's body, labels, or status using the Rust CLI
- **THEN** the existing commit references remain attached to the task

### Requirement: Expose Git traceability without enforcing project workflow

Todoing MUST treat commit references as task traceability metadata.

The presence of a commit reference MUST NOT by itself be treated as proof that a task is complete. The optional bundled Git completion hook may require and validate commit references when that hook is installed.

#### Scenario: Treat commit references as traceability metadata

- **WHEN** a task has one or more linked commit references
- **THEN** Todoing exposes those references as traceability metadata
- **AND** does not treat their presence alone as proof that the task is complete
