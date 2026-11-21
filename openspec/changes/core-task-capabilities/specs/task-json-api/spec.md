## ADDED Requirements

### Requirement: Versioned machine-readable interface

The system MUST provide a documented, versioned JSON interface for supported task operations. The interface MUST remain backward-compatible within a declared API version.

#### Scenario: Identify the API version

- **WHEN** a client invokes the machine interface
- **THEN** the response or negotiated interface metadata identifies the applicable API version

### Requirement: Deterministic JSON output

The system MUST provide deterministic machine-readable JSON representations for supported operations. JSON output MUST be written to stdout without human-readable diagnostics mixed into the JSON stream.

#### Scenario: Request task data

- **WHEN** a client requests task data through the JSON interface
- **THEN** the response conforms to the documented schema for the declared API version
- **AND** diagnostics are kept separate from the JSON response

### Requirement: Stable machine-readable errors

The system MUST expose stable machine-readable error codes and predictable process exit codes for failed CLI operations.

#### Scenario: Handle a rejected operation

- **WHEN** a CLI operation is rejected because of invalid input, a missing task, a lock conflict, or a storage failure
- **THEN** the process returns the documented exit code
- **AND** the machine-readable error identifies a stable error code
