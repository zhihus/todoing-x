# CLI and JSON contract

## Requirements

### Requirement: Primary command-line interface

The system MUST provide a command-line interface as its primary human and agent interface.

The CLI MUST expose operations for task creation, inspection, mutation, configured status workflows, dependencies, history and version inspection, and lock acquisition, renewal, and release.

#### Scenario: Perform a supported task operation

- **WHEN** a client invokes a documented CLI operation with valid arguments
- **THEN** the operation is performed according to the relevant capability requirements

### Requirement: Versioned machine-readable JSON contract

The CLI MUST provide a documented, versioned JSON contract for machine clients.

The contract MUST define deterministic response structures and MUST remain backward-compatible within a declared contract version. A breaking contract change MUST use an explicitly declared new version.

#### Scenario: Request machine-readable task data

- **WHEN** a client invokes a command using the documented JSON contract
- **THEN** the CLI returns a response conforming to the declared contract version

### Requirement: Stable machine-readable errors

The CLI MUST return stable machine-readable error codes and predictable process exit codes for validation errors, missing resources, lock conflicts, dependency conflicts, and persistence failures.

Diagnostics MUST NOT corrupt JSON output on stdout.

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
