# CLI and JSON API

## ADDED Requirements

### Requirement: Provide a stable machine-readable JSON contract

Todoing MUST provide a documented, versioned JSON interface for machine clients. The contract MUST define deterministic response structures and MUST remain backward-compatible within a declared contract version.

#### Scenario: Request a response using a supported contract version

- **WHEN** a client requests task data through a supported JSON contract version
- **THEN** Todoing returns a response conforming to that version's documented schema

### Requirement: Provide stable machine-readable errors

Todoing MUST expose stable machine-readable error codes and predictable process exit codes for CLI operations.

Diagnostics MUST NOT corrupt JSON output written to standard output.

#### Scenario: Handle a failed JSON operation

- **WHEN** a task operation fails while the client requests JSON output
- **THEN** Todoing returns the documented machine-readable error representation
- **AND** exits with the documented process exit code
- **AND** writes diagnostics only to the designated diagnostic stream

### Requirement: Expose core task capabilities through the CLI and JSON interface

The public client interface MUST expose operations for task management, status configuration, dependencies, task history and versions, and task lock management.

#### Scenario: Use a core task operation as a machine client

- **WHEN** a machine client requests a supported core task operation
- **THEN** Todoing processes the request through the documented public interface
- **AND** returns a response conforming to the versioned JSON contract

The exact command set, JSON envelope, version negotiation, and error-code catalogue remain architecture decisions.
