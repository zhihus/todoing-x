# Task CLI

## Purpose

Describe the user-visible commands and machine-readable output currently provided by the Todoing CLI.

## Requirements

### Requirement: Create tasks

The CLI MUST provide an `add` command that creates a task with a title and optional labels, status, and description body.

A title MUST contain a non-whitespace character. Supplied labels MUST contain a non-whitespace character. The accepted status values are `todo`, `in_progress`, `done`, and `cancelled`. Status values supplied to CLI commands are case-insensitive.

#### Scenario: Create a valid task

- **WHEN** the user creates a task with a valid title and supported optional values
- **THEN** the system assigns an identifier and creation timestamp
- **AND** persists the task

#### Scenario: Reject invalid task input

- **WHEN** the title or a supplied label is empty or whitespace-only, or the status is unsupported
- **THEN** the command fails
- **AND** the invalid task is not persisted

### Requirement: List tasks

The CLI MUST provide an `ls` command that lists tasks in descending task-ID order by default. The `--reverse` option MUST list tasks in ascending task-ID order.

#### Scenario: List tasks in default order

- **WHEN** the user runs `todoing ls` without `--reverse`
- **THEN** tasks are listed in descending task-ID order

#### Scenario: Reverse task order

- **WHEN** the user runs `todoing ls --reverse`
- **THEN** tasks are listed in ascending task-ID order

### Requirement: Filter tasks

The `ls` command MUST support filtering by status, one or more labels, and a case-insensitive query. Query matching MUST use substring matching for title, body, and labels, and exact matching for the status value. Multiple label filters MUST use AND semantics.

#### Scenario: Filter by multiple labels

- **WHEN** the user supplies more than one label filter
- **THEN** only tasks containing every requested label are returned
- **AND** label comparison is case-insensitive

#### Scenario: Search task fields

- **WHEN** the user supplies a query
- **THEN** the CLI performs case-insensitive substring matching against title, body, and labels
- **AND** matches status only when the query equals the complete status value

### Requirement: Limit and scan task results

The `ls` command MUST support a result limit. A positive limit truncates the result after sorting and filtering; zero or a negative limit means no limit.

The `--scan` option MUST scan task Markdown files and rebuild `.todoing/index.json` before listing. The listing is then produced from the rebuilt index; this option is not a read-only operation.

#### Scenario: Apply a positive result limit

- **WHEN** the user supplies a positive limit
- **THEN** the sorted and filtered task result is truncated to that number of tasks

#### Scenario: List after scan

- **WHEN** the user runs `todoing ls --scan`
- **THEN** the system rebuilds the JSON index from task Markdown files
- **AND** lists tasks using the rebuilt index

### Requirement: Inspect and edit task content

The CLI MUST provide a `see` command to show a task, an `edit` command to open its Markdown file in the configured editor, `body` to replace its description, `append` to append to its description, `status` to change its status, and `label` to add or remove labels.

Task operations MUST accept a numeric task identifier, optionally prefixed with `#`.

#### Scenario: Show a task

- **WHEN** the user runs `todoing see` with an existing task ID
- **THEN** the task is displayed with its Markdown frontmatter and body

#### Scenario: Edit a task in the configured editor

- **WHEN** the user runs `todoing edit` with an existing task ID
- **THEN** the task's Markdown file is opened using the configured editor

#### Scenario: Replace or append task body

- **WHEN** the user runs `todoing body` or `todoing append` for an existing task
- **THEN** the task body is replaced or appended respectively
- **AND** the updated task is persisted

#### Scenario: Change status or labels

- **WHEN** the user runs `todoing status` or `todoing label` for an existing task
- **THEN** the requested status or label changes are persisted

### Requirement: Link Git commits

The CLI MUST provide a `commit` command to link a Git revision to a task.

#### Scenario: Link a revision

- **WHEN** the user supplies a valid Git commit revision for an existing task
- **THEN** the system resolves the revision to a full commit SHA
- **AND** stores the reference with the task

### Requirement: Delete tasks

The current CLI MUST provide a `delete` command that deletes a task after confirmation, unless the user supplies the force option.

This requirement records current behavior. It does not assert that physical deletion is part of the accepted target product direction.

#### Scenario: Abort deletion

- **WHEN** the user declines the confirmation or confirmation input ends
- **THEN** the task remains stored

#### Scenario: Confirm deletion

- **WHEN** the user confirms deletion or supplies the force option
- **THEN** the task file and corresponding index entry are deleted

### Requirement: JSON output

The CLI MUST support a global `--json` option for machine-readable command output where the command provides a JSON representation.

The command's JSON result MUST be written to stdout. Command diagnostics and errors MUST be written to stderr.

The current JSON interface is not yet a formally versioned API contract.

#### Scenario: Request task data as JSON

- **WHEN** the user invokes a command that supports JSON output with `--json`
- **THEN** the command emits its machine-readable result as JSON on stdout
