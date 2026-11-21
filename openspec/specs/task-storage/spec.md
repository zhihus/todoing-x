# Task storage

## Purpose

Describe the task persistence behavior implemented by the current Todoing CLI.

## Requirements

### Requirement: Project-local task store

The system MUST store task data under the `.todoing/` directory of the current working project.

#### Scenario: First task operation

- **WHEN** a task operation requires storage and the project has no Todoing store
- **THEN** the system creates the required `.todoing/tasks/` directory
- **AND** creates or rebuilds `.todoing/index.json` when the operation requires the index

### Requirement: Markdown task records

The system MUST persist each task as a Markdown file under `.todoing/tasks/`, named with its task identifier and the `.md` extension.

Task records use YAML frontmatter and a Markdown body. The task identifier and title are required. If omitted, status defaults to `todo`, labels and commit references default to empty collections, the creation timestamp defaults to the current local time, and the description body defaults to empty.

#### Scenario: Read and rewrite a task

- **WHEN** a task file contains valid supported frontmatter fields
- **THEN** the system parses the task fields
- **AND** can rewrite the task in the Todoing Markdown representation

The current implementation trims leading and trailing whitespace from the description body when reading and writing a task. Frontmatter fields not represented by the current task model are not preserved when the task is rewritten.

#### Scenario: Skip a task file without usable frontmatter

- **WHEN** a scanned Markdown file has no opening frontmatter delimiter, no closing delimiter, or frontmatter that cannot be parsed as YAML
- **THEN** the file is skipped during index rebuild
- **AND** a warning is emitted when the opening delimiter is missing

#### Scenario: Reject invalid task fields

- **WHEN** a scanned file has non-mapping frontmatter or an invalid or missing required field, or an invalid supplied status, label, commit reference, or creation timestamp
- **THEN** index rebuild fails with an error identifying the task file

Unreadable files caused by invalid UTF-8 produce an error. Other file-read errors are skipped by the current implementation.

### Requirement: Rebuildable JSON index

The system MUST maintain a JSON index at `.todoing/index.json` for task listing and lookup.

The index MUST be rebuildable from valid Markdown task files.

#### Scenario: Rebuild index

- **WHEN** the user runs `todoing reindex`
- **THEN** the system scans task Markdown files in ascending numeric filename order
- **AND** rewrites the JSON index from the task files it can parse
- **AND** fails if a scanned file contains invalid required or supplied task fields

#### Scenario: Recover a missing or structurally invalid index

- **WHEN** the index is missing, contains invalid JSON, is not a JSON array, or contains an item missing its `id` or `title` key
- **THEN** the system rebuilds the index from task Markdown files

An index item that contains the required keys but has invalid field values produces an error instead of triggering a rebuild.

#### Scenario: Scan before listing

- **WHEN** the user runs `todoing ls --scan`
- **THEN** the system scans task Markdown files and rebuilds `.todoing/index.json`
- **AND** produces the listing from the rebuilt index

### Requirement: Atomic replacement of individual files

The system MUST write an individual task or index file through a temporary file in the destination directory and replace the destination only after the temporary file has been written and synchronized.

This requirement describes individual file replacement only. It does not establish atomicity across a task file and the JSON index as one logical transaction.

#### Scenario: Successful file replacement

- **WHEN** the system successfully writes a task or index file
- **THEN** the destination contains the complete new file content
- **AND** the replacement operation does not expose partially written destination content

#### Scenario: Failure between task and index writes

- **WHEN** a task file is successfully written but the subsequent index update fails
- **THEN** the task file may contain the new task state while the index remains stale
- **AND** the index can be rebuilt from task Markdown files

### Requirement: Existing storage compatibility

The system MUST continue to read the current Markdown/YAML task representation and JSON index format used by the Rust port.

#### Scenario: Existing task without optional commit references

- **WHEN** a valid task record does not contain a `commits` field
- **THEN** the system reads it with an empty commit-reference collection
