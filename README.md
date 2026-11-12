# todoing (Rust)

Rust implementation of [the original Python todoing](https://github.com/bllyanos/todoing).

The Python project is the compatibility baseline: existing commands, task files, index format, status model, and normal output remain supported. Rust adds validation, atomic writes, a JSON interface, Git commit references, and lifecycle gates.

## Install

Requires Rust and Cargo. Install directly from GitHub:

```bash
cargo install --git https://github.com/zhihus/todoing-x
```

Verify with `todoing --help` and `todoing version`. Cargo's binary directory (`~/.cargo/bin` or `%USERPROFILE%\\.cargo\\bin`) must be on `PATH`.

## Build

```bash
cargo build --release
cargo test
```

## Agent skills

The repository includes two skills under `.agents/skills/`:

- `todoing` — CLI reference and task-management workflow for AI agents.
- `todoing-install` — installation and project-onboarding instructions.

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

## Task creation validation

Validation runs only when creating a task, before the task file or index is written:

- `title` is required and cannot be empty or whitespace-only.
- `status` must be one of the four supported statuses.
- Every supplied label must contain a non-whitespace character.
- The ID and creation timestamp are generated automatically; `body` and labels are optional.

After built-in validation, the optional `on-create` hook can apply project-specific checks to the complete task JSON. A rejected task is not saved.

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

The command resolves the supplied revision to a full commit SHA and stores it in the task frontmatter and JSON index. Existing task files without commit metadata remain valid. The original Python implementation does not preserve this extension. Any Python command that writes `index.json` drops commit references from the index for all tasks; run `todoing reindex` with the Rust version to restore them from Markdown files. If Python rewrites a task that has linked commits, it also removes those references from that task's Markdown file, and they cannot be recovered. Use the Rust version whenever commit links are in use.

## Lifecycle hooks

Todoing bundles two standard completion hooks, but does not install them automatically. The user chooses which checks to enable in each project:

```bash
todoing hooks status --json
todoing hooks install git
todoing hooks install openspec
todoing hooks install all
```

- `before-done-git` — requires at least one linked Git commit and verifies that every linked SHA exists in the current repository.
- `before-done-openspec` — checks that all checklist items in active `openspec/changes/*/tasks.md` files are complete; `archive/` is ignored.
- `before-done` — additional project-specific check, supplied by the project.
- `on-create` — optional project-specific check before a new task is written.

`hooks install` installs only the selected standard hook files and never overwrites an existing file. On Windows it creates `.cmd` files; on Unix-like systems it creates executable files. To disable a standard hook, remove its file from `.todoing/hooks/`. Missing hooks are no-ops. The bundled files are small launchers for checks implemented by the Todoing binary.

The task JSON is sent to each hook's standard input. Completion hooks run in this order: `before-done-git`, `before-done-openspec`, then `before-done`. Exit code 0 allows the operation; any non-zero exit code rejects it. The same completion hooks run when creating a task directly with `--status done`.

AI agents must run `todoing hooks status --json` before managing tasks in a project. If standard hooks are missing, explain each check and ask the user which, if any, to install. Never install hooks without explicit approval.

## Storage and safety

```
.todoing/
  tasks/
    1.md
    2.md
  index.json
  hooks/
    on-create
    before-done-git       # installed optionally
    before-done-openspec  # installed optionally
    before-done           # project-specific
```

Task files remain Markdown with YAML frontmatter. `index.json` remains a rebuildable JSON array. Commit the entire `.todoing/` directory to Git so task files and their index are versioned with the project. Both task and index writes use a temporary file in the destination directory and atomic replacement to avoid partially written files.

The four original statuses and user-controlled task queue are unchanged. No priorities, dependencies, event history, or automatic archiving are added.

## Compatibility status

The Rust port is under active development. A passing Rust test suite does not by itself certify complete behavioral equivalence with the Python implementation.
