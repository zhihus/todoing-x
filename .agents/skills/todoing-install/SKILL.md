---
name: todoing-install
description: Guide users through installing the Rust-based todoing CLI and setting it up in a project so AI agents can use it for task tracking.
---

# todoing-install

A guide for installing the Rust `todoing` CLI and integrating it into a project.

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

## Troubleshooting

| Symptom | Likely fix |
|---|---|
| `todoing: command not found` | Add Cargo's binary directory to `PATH`; reopen the terminal after changing it. |
| `cargo: command not found` | Install the Rust toolchain with rustup, then reopen the terminal. |
| `linker/compiler errors` | Install the platform's native build tools and retry. |
| `Error: No such command` | Check `todoing --help` and update with `cargo install --git https://github.com/zhihus/todoing-x --force`. |

## Set up the agent skill

The repository contains the skill at `.agents/skills/todoing/`. If the repository is already available locally, the skill is ready to use.

For another repository, copy the skill directory:

```bash
mkdir -p /path/to/your/repo/.agents/skills
cp -r /path/to/todoing-x/.agents/skills/todoing /path/to/your/repo/.agents/skills/
```

The skill can then be loaded by agents that support the `.agents/skills/` convention.

## Add todoing instructions to the project

Ask the user before modifying project-level instructions such as `AGENTS.md` or `CLAUDE.md`. If approved, add a short section such as:

```markdown
## Task management with todoing

This project uses the Rust [todoing](https://github.com/zhihus/todoing-x) CLI.
Tasks live in `.todoing/tasks/`. Use the CLI rather than editing task files
directly, except when diagnosing a broken store.

Quick reference:
- `todoing ls -n 5` — inspect recent tasks
- `todoing add "title"` — create a task
- `todoing status <id> in_progress` — start work
- `todoing append <id> "progress"` — record progress
- `todoing commit <id> HEAD` — link a Git commit
- `todoing status <id> done` — complete a task after completion checks pass
```

Place the instructions in the project's root `AGENTS.md` or `CLAUDE.md`, whichever the user prefers or already uses. Do not modify project instructions without the user's approval.
