# Storage engine evaluation spike

This isolated crate compares SQLite via rusqlite and redb for the Todoing MVP's core persistence requirements.

It is a research prototype, not production code and not part of the Todoing runtime dependency graph.

## Covered checks

- One logical task creation writes task, version, and history records in a single transaction.
- An injected failure after the task record rolls back all three records.
- Separate OS processes create tasks concurrently without lost records or duplicate sequence allocation.
- A child process exits without running Rust destructors after writing the task record but before commit; the next process must recover a consistent pre-commit state.

The process-exit test is not a physical power-loss test. It does not replace crash-injection testing against the filesystem and storage device.

## Run

From the repository root:

```sh
cargo test --manifest-path spikes/storage-evaluation/Cargo.toml
```

The GitHub Actions workflow runs the spike tests on Ubuntu and Windows.

## Deliberate limitations

- The schema is intentionally minimal and exists only to test transaction boundaries.
- It does not implement the full task model, dependencies, status configuration, history semantics, or task-lock protocol.
- It does not measure performance.
- A passing spike does not by itself select a production storage engine.
