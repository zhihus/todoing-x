# todoing (Rust)

Rust implementation of [the original Python todoing](https://github.com/bllyanos/todoing).

The Python project is the compatibility baseline: existing commands, task files, index format, status model, and normal output remain supported. Rust adds validation, atomic writes, a JSON interface, Git commit references, and lifecycle gates.

## Build

```bash
cargo build --release
cargo test
```

## Compatible CLI

```bash
todoing add "Ship the MVP"
todoing add "Write integration tests" -l testing -s in_progress
todoing ls
todoing ls -n 5
todoing ls -s in_progress
todoing ls -l p0 -l backend
todoing ls -q auth
todoing ls -r
todoing see 4
todoing status 4 in_progress
todoing label 4 +p0
todoing label 4 -- -backend
todoing body 4 "Replace description"
todoing append 4 "Add details"
todoing delete 4 --force
todoing edit 4
todoing reindex
todoing version
```

Statuses are unchanged: `todo`, `in_progress`, `done`, `cancelled`.

## AI-agent interface

Pass `--json` before or after a command to receive machine-readable JSON:

```bash
todoing --json ls
todoing ls --json -s in_progress
todoing --json see 4
todoing --json add "Implement validation"
todoing --json status 4 in_progress
```

JSON output is written to stdout. Diagnostics and errors are written to stderr.

## Git commit references

Link a local Git commit to a task:

```bash
todoing commit 4 HEAD
todoing commit 4 a1b2c3d
todoing --json see 4
```

The command resolves the supplied revision to a full commit SHA and stores it in the task frontmatter and JSON index. Existing task files without commit metadata remain valid.

## Lifecycle hooks

Optional executable hooks live in `.todoing/hooks/`:

- `on-create` — invoked before a new task is written.
- `before-done` — invoked before a task transitions to `done`.

The task JSON is sent to the hook's standard input. Exit code 0 allows the operation; any non-zero exit code rejects it. On Windows, use `.cmd` or `.bat` suffixes; on Unix-like systems, use executable files named exactly `on-create` and `before-done`.

Before `done`, todoing also enforces these built-in checks:

1. At least one Git commit must be linked to the task.
2. Every commit reference must resolve to a commit in the current repository.
3. If `openspec/changes/` exists, every checklist item in each active change's `tasks.md` must be checked. The `archive/` directory is ignored.

A missing custom hook is a no-op; the built-in completion checks still run.

## Storage and safety

```
.todoing/
  tasks/
    1.md
    2.md
  index.json
  hooks/
    on-create
    before-done
```

Task files remain Markdown with YAML frontmatter. `index.json` remains a rebuildable JSON array. Both task and index writes use a temporary file in the destination directory and atomic replacement to avoid partially written files.

The four original statuses and user-controlled task queue are unchanged. No priorities, dependencies, event history, or automatic archiving are added.

## Compatibility status

The Rust port is under active development. A passing Rust test suite does not by itself certify complete behavioral equivalence with the Python implementation.
