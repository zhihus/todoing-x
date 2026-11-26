# Protect private project data from Git

## Why

Todoing stores personal task data under the project-local `.todoing/` directory. The current repository documentation incorrectly recommends committing the project configuration and database, and the application does not ensure that runtime data is ignored by Git.

## What changes

- Treat the complete `.todoing/` directory as private local data that must never be committed.
- Make `init` and `migrate` establish and verify a Git ignore rule before creating or activating data.
- Refuse to proceed if any `.todoing/` path is already tracked; do not untrack or rewrite history automatically.
- Correct product and user documentation.
- Add regression tests for ignore setup and tracked-data refusal.

## Non-goals

- No task-data synchronization, online database, replication, or hybrid mode.
- No automatic deletion or untracking of existing files.
- No changes to task storage format or application workflow.
