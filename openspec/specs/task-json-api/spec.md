# Task JSON API

## Purpose

Describe the versioned machine-readable interface provided by the Todoing CLI and SQLite task repository.

## Requirements

### Requirement: Versioned single-request interface

The system MUST provide a documented JSON API v1. Each `todoing api` process MUST read one request object from stdin and emit exactly one response object to stdout.

Every request MUST include `api_version`, `project_id`, `actor_id`, `operation`, and `args`. `request_id` is optional and MUST be echoed unchanged when supplied. Unknown envelope properties, unknown operation names, invalid argument properties, and unsupported API versions MUST be rejected.

#### Scenario: Process a valid API v1 request

- **WHEN** a client submits a valid request with API version 1 and a matching project ID
- **THEN** the process executes the requested operation
- **AND** emits exactly one response with `api_version: 1` and `ok: true`
- **AND** echoes `request_id` when supplied

#### Scenario: Reject an unsupported API version

- **WHEN** a client submits an unsupported API version
- **THEN** the process returns an `unsupported_api_version` error
- **AND** no operation is executed

#### Scenario: Reject a mismatched project ID

- **WHEN** a client submits a project ID that differs from the current store
- **THEN** the operation is rejected with `project_id_mismatch`
- **AND** no task data is returned or changed

### Requirement: Deterministic JSON and error output

JSON responses, including errors, MUST be written to stdout. Diagnostics MUST be written to stderr and MUST NOT be mixed into the JSON response. Failed operations MUST return non-zero process exit codes mapped to stable error classes.

#### Scenario: Handle a rejected operation

- **WHEN** an operation is rejected because of invalid input, a missing task, a lock conflict, or a storage failure
- **THEN** the response contains a stable machine-readable error code
- **AND** the process returns the documented exit code

### Requirement: Expose task operations

The API MUST expose store info, integrity verification, migration, snapshots, task create/list/get/update/status, complete/cancel/reopen, labels, Git commit references, dependencies, history, versions, baseline approval, and task lock operations.

#### Scenario: Migrate through the API

- **WHEN** a client invokes `store.migrate`
- **THEN** the API returns a migration report
- **AND** supports a non-mutating dry run

#### Scenario: Create a snapshot through the API

- **WHEN** a client invokes `store.snapshot`
- **THEN** the API returns the verified snapshot path, size, and SHA-256 checksum

#### Scenario: Cancel a task through the API

- **WHEN** a client submits a valid cancellation request
- **THEN** the task outcome becomes cancelled
- **AND** workflow status remains unchanged
- **AND** history records the actor and optional reason

### Requirement: Archive and restore tasks

The API MUST expose `task.archive` and `task.restore`, each requiring `task_id`. `task.list` MUST accept `archive_state` values `active` (default), `archived`, and `all`. Task records MUST include archive metadata. Repeated archive/restore requests MUST return `no_op=true`, append a no-op history event, and MUST NOT create a task version.

#### Scenario: Filter and restore archived tasks

- **WHEN** a client archives a task and requests `task.list` with `archive_state=archived`
- **THEN** the task is returned only in the archive view
- **AND** `task.restore` returns it to the default active list without changing workflow status or outcome

#### Scenario: Reject mutation of an archived task

- **WHEN** a client attempts an ordinary mutation or new lock acquisition for an archived task
- **THEN** the API returns `task_archived`

### Requirement: Enforce project-local task access

The project ID MUST match the active store metadata. Actor identity MUST be supplied for reads and writes and recorded for attribution; actor identity is not an authentication mechanism.

#### Scenario: Attribute an operation

- **WHEN** a client submits a valid request with an actor ID
- **THEN** successful and applicable rejected operation events record that actor
