# CLI and JSON contract

## Requirements

### Requirement: Primary command-line interface

The system MUST provide a command-line interface as its primary human and agent interface.

The CLI MUST expose operations for task creation, inspection, mutation, configured status workflows, dependencies, history and version inspection, approval/baseline selection, migration, and lock acquisition, renewal, and release.

The CLI SHOULD preserve existing command names where practical. The physical delete operation MUST NOT delete task data. If the legacy delete command is retained for compatibility, it MUST return `PHYSICAL_DELETE_UNSUPPORTED`.

#### Scenario: Perform a supported task operation

- **WHEN** a client invokes a documented CLI operation with valid arguments
- **THEN** the operation is performed according to the relevant capability requirements

#### Scenario: Attempt physical deletion

- **WHEN** a client invokes the legacy physical delete command
- **THEN** the CLI returns `PHYSICAL_DELETE_UNSUPPORTED`
- **AND** the task and its history remain unchanged

### Requirement: Versioned machine-readable JSON contract

The CLI MUST provide a documented, versioned JSON contract for machine clients.

Every JSON response MUST include `api_version` and `ok`. A successful response MUST include `data`; a failed response MUST include an `error` object containing a stable `code` and a human-readable `message`.

The contract MUST remain backward-compatible within a declared contract version. A breaking contract change MUST use an explicitly declared new version.

#### Scenario: Request machine-readable task data

- **WHEN** a client invokes a command using the documented JSON contract
- **THEN** the CLI returns a response conforming to the declared contract version

### Requirement: Stable machine-readable errors

The CLI MUST return stable machine-readable error codes and predictable process exit codes for validation errors, missing resources, lock conflicts, dependency conflicts, unsupported deletion, migration failures, and persistence failures.

Diagnostics MUST NOT corrupt JSON output on stdout.

The initial error codes MUST include:

- `INVALID_ARGUMENT`
- `TASK_NOT_FOUND`
- `STATUS_NOT_FOUND`
- `INVALID_TRANSITION`
- `DEPENDENCY_NOT_FOUND`
- `DEPENDENCY_CYCLE`
- `TASK_LOCKED`
- `LOCK_NOT_OWNER`
- `LOCK_EXPIRED`
- `VERSION_NOT_FOUND`
- `PHYSICAL_DELETE_UNSUPPORTED`
- `CONFLICT`
- `MIGRATION_ERROR`
- `STORAGE_ERROR`
- `INTERNAL_ERROR`

The initial process exit codes MUST be:

| Code | Meaning |
|---:|---|
| 0 | Success |
| 2 | CLI usage or argument parsing error |
| 3 | Validation or invalid transition |
| 4 | Requested resource not found |
| 5 | State or dependency conflict |
| 6 | Lock conflict or owner authorization failure |
| 7 | Storage, integrity, or migration failure |
| 8 | Unexpected internal failure |

#### Scenario: Handle a machine-readable error

- **WHEN** a command fails with a documented error condition and JSON output is requested
- **THEN** the CLI returns the corresponding stable error code
- **AND** exits with the documented process exit code
- **AND** keeps diagnostic text separate from JSON stdout

### Requirement: Preserve human usability

The CLI MUST retain a human-readable output mode for interactive use.

#### Scenario: Run an interactive command

- **WHEN** a human invokes a command without requesting JSON output
- **THEN** the CLI presents a readable result or diagnostic appropriate to the operation
