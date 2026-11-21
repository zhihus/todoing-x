# Task storage

## Purpose

Describe the task persistence behavior implemented by the current Todoing CLI.

## Requirements

### Requirement: Project-local task store

The system MUST store task data under the `.todoing/` directory of the current working project.

#### Scenario: First task operation

- **WHEN** a task operation requires storage and the project has no Todoing store
- **THEN** the system creates the required `.todoing/tasks/` directory and storage files as needed

### Requirement: Markdown task records

The system MUST persist each task as a Markdown file under `.todoing/tasks/`, named with its task identifier and the `.md` extension.

Each task record MUST represent the task identifier, title, creation timestamp, status, labels, description body, and optional Git commit references using YAML frontmatter and Markdown body content.

#### Scenario: Read an existing task

- **WHEN** a task file contains valid YAML frontmatter and supported field values
- **THEN** the system parses the supported task fields
- **AND** can rewrite those fields in the Todoing Markdown representation

The current implementation normalizes the description body by trimming its leading and trailing whitespace when reading a task. Frontmatter fields not represented by the current task model are not preserved when the task is rewritten.

#### Scenario: Invalid or unreadable task file

- **WHEN** a task file cannot be parsed as a valid task
- **THEN** the system skips some malformed or incomplete files during index rebuild, or returns an error for invalid required fields and unreadable content, according to the failure condition

### Requirement: Rebuildable JSON index

The system MUST maintain a JSON index at `.todoing/index.json` for task listing and lookup.

The index MUST be rebuildable from valid Markdown task files.

#### Scenario: Rebuild index

- **WHEN** the user runs `todoing reindex`
- **THEN** the system scans task Markdown files
- **AND** rewrites the JSON index from the tasks it can parse
- **AND** fails if a scanned file has invalid required task fields

#### Scenario: Scan before listing

- **WHEN** the user runs `todoing ls --scan`
- **THEN** the system scans task Markdown files and rebuilds the JSON index
- **AND** produces the listing from the rebuilt index

### Requirement: Atomic replacement of individual files

The system MUST write an individual task or index file through a temporary file in the destination directory and replace the destination only after the temporary file has been written and synchronized.

This requirement describes individual file replacement only. It does not establish atomicity across a task file and the JSON index as one logical transaction.

#### Scenario: Successful file replacement

- **WHEN** the system successfully writes a task or index file
- **THEN** the destination contains the complete new file content
- **AND** no partially written destination content is exposed by the replacement operation

#### Scenario: Failure between task and index writes

- **WHEN** a task file is successfully written but the subsequent index update fails
- **THEN** the task file may contain the new task state while the index remains stale
- **AND** the index can be rebuilt from task Markdown files

### Requirement: Existing storage compatibility

The system MUST continue to read the current Markdown/YAML task representation and JSON index format used by the Rust port.

#### Scenario: Existing task without optional commit references

- **WHEN** a valid task record does not contain a `commits` field
- **THEN** the system reads it with an empty commit-reference collection
