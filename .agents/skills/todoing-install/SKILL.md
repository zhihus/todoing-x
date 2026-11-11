---
name: todoing-install
description: Guide users through installing and configuring the Rust-based todoing CLI, and onboarding AI agents to use its task-management skill.
---

# todoing-install

Use this guide to install the Rust `todoing` CLI and make its agent skill available in a project.

## Requirements

- Rust toolchain with Cargo.
- Git.

## Install

Install the latest version from the Rust repository:

```bash
cargo install --git https://github.com/zhihus/todoing-x
```

Verify:

```bash
todoing --help
todoing version
```

To update an existing installation:

```bash
cargo install --git https://github.com/zhihus/todoing-x --force
```

Cargo normally places installed binaries in `~/.cargo/bin` (Unix-like systems) or `%USERPROFILE%\\.cargo\\bin` (Windows). Ensure that directory is on `PATH`.

## Install the agent skill

The repository contains the skill at `.agents/skills/todoing/`. If the repository is already available locally, copy it into the target project's skills directory:

```bash
mkdir -p /path/to/your/repo/.agents/skills
cp -r /path/to/todoing-x/.agents/skills/todoing /path/to/your/repo/.agents/skills/
```

The skill can then be loaded by agents that support the `.agents/skills/` convention.

## Configure project instructions

Ask the user before modifying project-level instructions such as `AGENTS.md` or `CLAUDE.md`. If approved, add a short section such as:

```markdown
## Task management with todoing

This project uses the Rust [todoing](https://github.com/zhihus/todoing-x) CLI.
Tasks live in `.todoing/tasks/`. Use the CLI rather than editing task files
directly, except when diagnosing a broken store.

- `todoing ls -n 5` — inspect recent tasks
- `todoing add "title"` — create a task
- `todoing status <id> in_progress` — start work
- `todoing append <id> "progress"` — record progress
- `todoing commit <id> HEAD` — link a Git commit
- `todoing status <id> done` — complete a task after completion checks pass
```

Do not modify project instructions without the user's approval.
