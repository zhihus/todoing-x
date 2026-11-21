# Task CLI

## Purpose

Describe the user-visible commands and machine-readable output currently provided by the Todoing CLI.

## Requirements

### Requirement: Create tasks

The CLI MUST provide an `add` command that creates a task with a title and optional labels, status, and description body.

A title MUST contain a non-whitespace character. Supplied labels MUST contain a non-whitespace character. The accepted status values are `todo`, `in_progress`, `done`, and `cancelled`.

#### Scenario: Create a valid task

- **WHEN** the user creates a task with a valid title and supported optional values
- **THEN** the system assigns an identifier and creation timestamp
- **AND** persists the task

#### Scenario: Reject invalid task input

- **WHEN** the title or a supplied label is empty or whitespace-only, or the status is unsupported
- **THEN** the command fails
- **AND** the invalid task is not persisted

### Requirement: List and filter tasks

The CLI MUST provide an `ls` command that lists tasks newest first by default.

The command MUST support filtering by status, one or more labels, and a case-insensitive query across title, body, labels, and status. Multiple label filters MUST use AND semantics. The command MUST support a result limit and reverse ordering.

The `--scan` option MUST scan task Markdown files and rebuild `.todoing/index.json` before listing. The listing is then produced from the rebuilt index; this option is not a read-only operation.

#### Scenario: Filter by multiple labels

- **WHEN** the user supplies more than one label filter
- **THEN** only tasks containing every requested label are returned

#### Scenario: List after scan

- **WHEN** the user runs `todoing ls --scan`
- **THEN** the system rebuilds the JSON index from task Markdown files
- **AND** lists tasks using the rebuilt index

### Requirement: Inspect and edit task content

The CLI MUST provide a `see` command to show a task, an `edit` command to open its Markdown file in the configured editor, `body` to replace its description, `append` to append to its description, `status` to change its status, and `label` to add or remove labels.

Task operations MUST accept a numeric task identifier, optionally prefixed with `#`.

### Requirement: Link Git commits

The CLI MUST provide a `commit` command to link a Git revision to a task.

#### Scenario: Link a revision

- **WHEN** the user supplies a valid Git revision for an existing task
- **THEN** the system resolves the revision to a full commit SHA
- **AND** stores the reference with the task

### Requirement: Delete tasks

The current CLI MUST provide a `delete` command that deletes a task after confirmation, unless the user supplies the force option.

This requirement records current behavior. It does not assert that physical deletion is part of the accepted target product direction.

#### Scenario: Abort deletion

- **WHEN** the user declines the confirmation or confirmation input ends
- **THEN** the task remains stored

### Requirement: JSON output

The CLI MUST support a global `--json` option for machine-readable command output.

JSON data MUST be written to stdout. Diagnostics and errors MUST be written to stderr.

The current JSON interface is not yet a formally versioned API contract.

#### Scenario: Request task data as JSON

- **WHEN** the user invokes a command that supports JSON output with `--json`
- **THEN** the command emits its machine-readable result as JSON on stdout
