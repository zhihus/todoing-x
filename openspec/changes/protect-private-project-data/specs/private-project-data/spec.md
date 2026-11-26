## ADDED Requirements

### Requirement: Private project data is excluded from Git

The complete project-local `.todoing/` directory MUST remain untracked and excluded from Git.

#### Scenario: Initialize inside a Git worktree

- **WHEN** a user initializes a Todoing store inside a Git worktree
- **THEN** Todoing adds and verifies a root-relative `/.todoing/` ignore rule before creating private files

#### Scenario: Migrate inside a Git worktree

- **WHEN** a user migrates legacy tasks inside a Git worktree
- **THEN** Todoing verifies the private directory is ignored before reading or writing migration data

### Requirement: Refuse operations when private data is tracked

Initialization and migration MUST stop when any path under `.todoing/` is already tracked by Git.

#### Scenario: Existing tracked private files

- **WHEN** Git reports tracked paths under `.todoing/`
- **THEN** Todoing refuses the operation without changing the index, deleting files, or rewriting history
