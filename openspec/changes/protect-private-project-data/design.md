# Design: private project data boundary

The complete project-local `.todoing/` directory is private runtime data. Git may version Todoing application source and development artifacts only.

A shared helper runs before initialization creates `.todoing/` and before migration reads or writes it:

1. Detect whether the project root is inside a Git worktree. Outside Git, continue without changing files.
2. Query the Git index for tracked paths under `.todoing/`.
3. If any are tracked, fail with an actionable error. Do not stage, unstage, delete, or rewrite history.
4. Ensure the project-root `.gitignore` contains the exact root-relative rule `/.todoing/`.
5. Verify with `git check-ignore` that a representative private file is ignored.
6. Continue only after the checks pass.

Ignore rules prevent ordinary accidental staging, not deliberate force-add. Existing history must be reviewed separately if private data was ever published. Consumers must not bypass the ignore rule.

Tests use temporary Git repositories to verify rule creation, ignore behavior, refusal when a private file is tracked, and no-op behavior outside a worktree.
