---
name: todoing
description: Use the `todoing` CLI for local task management. Track, list, search, and update tasks stored as Markdown files in `.todoing/tasks/`. Use this whenever the user asks to manage work items, track tasks, list to-dos, or mentions todoing in the context of task tracking.
---

# todoing

`todoing` is a local CLI task manager. Tasks live in `.todoing/tasks/{id}.md`, with a rebuildable JSON index at `.todoing/index.json`. Everything is Git-trackable plain text.

Run commands from the repository root so the correct `.todoing/` directory is used. It is created automatically when needed.

## Statuses

| Key | Meaning |
|---|---|
| `todo` | Not yet started |
| `in_progress` | Being worked on |
| `done` | Completed |
| `cancelled` | No longer needed |

IDs are sequential integers. `#1` and `1` are equivalent.

## Commands

### Create a task

```bash
todoing add "Set up CI pipeline"
todoing add "Fix API bug" -l bug -s in_progress -b "Reproduce the failure."
```

Options: repeatable `-l, --label`, `-s, --status` (default `todo`), and `-b, --body`.

Creation validation rejects an empty title or empty labels. The optional `.todoing/hooks/on-create` hook runs before the task is saved.

### List and search

```bash
todoing ls
todoing ls -n 5
todoing ls -s in_progress
todoing ls -l bug -l backend
todoing ls -q "connection timeout"
todoing ls -r
todoing ls --scan
```

- `-n, --limit`: number of newest tasks; `0` means all.
- `-s, --status`: filter by status.
- `-l, --label`: repeatable; all requested labels must match (AND).
- `-q, --query`: case-insensitive search in title, body, labels, and status.
- `-r, --reverse`: oldest first.
- `--scan`: rebuild the index from Markdown files before listing.

### Inspect and edit

```bash
todoing see 3
todoing body 3 "Replace the description."
todoing append 3 "Add another progress note."
todoing edit 3
todoing status 3 in_progress
todoing label 3 +bug -- -obsolete
todoing label 3 +urgent --clear
todoing delete 3 --force
todoing reindex
todoing version
```

Use `body` to replace the description and `append` to add a paragraph. `edit` opens the task file in `$EDITOR`; use `body`/`append` in non-interactive agent workflows. Label operations use `+label` to add and `-label` to remove; put `--` before removal operations when required by the CLI parser.

### Link Git commits

```bash
todoing commit 3 HEAD
todoing commit 3 a1b2c3d
```

The supplied revision is resolved to a full commit SHA and stored with the task.

## AI-agent interface

Use `--json` before or after a command for machine-readable output:

```bash
todoing --json ls
todoing ls --json -s in_progress
todoing --json see 3
todoing --json add "Implement validation"
```

JSON is written to stdout; errors are written to stderr.

## Completion checks and hooks

Before a task transitions to `done`, todoing requires at least one linked Git commit, verifies that every linked commit exists in the current repository, and checks that all checklist items in active `openspec/changes/*/tasks.md` files are complete. The `openspec/changes/archive/` directory is ignored.

Optional hooks are stored in `.todoing/hooks/`:
- `on-create`: receives the new task as JSON on stdin before it is saved.
- `before-done`: receives the task as JSON on stdin before a transition to `done`.

Exit code 0 allows the operation; a non-zero exit code rejects it. On Unix, use executable files named exactly `on-create` and `before-done`. On Windows, use `.cmd` or `.bat` files.

## Agent workflow

```bash
todoing ls -s todo -n 10
todoing status 3 in_progress
todoing append 3 "Investigating the failure."
# Make changes and run the required checks.
todoing commit 3 HEAD
todoing status 3 done
```

- Use the CLI rather than reading `.todoing/` files directly. Read files directly only to diagnose a broken store or CLI.
- Use `--force` with `delete` to avoid an interactive prompt.
- Record useful progress and handoff context in the task body.
- Do not mark a task done until its work is complete and the built-in completion checks and configured hook pass.
