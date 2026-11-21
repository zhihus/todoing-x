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

### Requirement: Expose task outcome operations

The versioned JSON interface MUST provide operations to complete, cancel, and reopen a task. Cancellation MUST accept an optional reason and MUST NOT require a dedicated workflow status.

#### Scenario: Cancel a task through the JSON interface

- **WHEN** a client submits a valid cancellation request for a task
- **THEN** the task outcome becomes cancelled
- **AND** the current workflow status remains unchanged
- **AND** the operation history records the actor, outcome, and optional reason

### Requirement: Use a single-request API envelope

The JSON API MUST accept one request object from stdin per process and emit exactly one response object to stdout. Every request MUST include api_version, project_id, actor_id, operation, and args. The response MUST echo request_id when supplied and MUST identify the API version.

#### Scenario: Process a valid API v1 request

- **WHEN** a client submits a valid request with api_version 1 and a matching project ID
- **THEN** the process executes the requested operation
- **AND** emits exactly one JSON response with api_version 1 and ok=true or ok=false
- **AND** echoes request_id when it was supplied

#### Scenario: Reject a mismatched project ID

- **WHEN** a client submits a request with a project ID that differs from the current store
- **THEN** the operation is rejected with the stable project_id_mismatch error
- **AND** no task data is returned or changed

#### Scenario: Reject an unsupported API version

- **WHEN** a client submits a request with an unsupported API version
- **THEN** the process returns the stable unsupported_api_version error
- **AND** no operation is executed
