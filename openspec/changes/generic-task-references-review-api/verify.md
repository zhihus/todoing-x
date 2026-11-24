# Verification: generic-task-references-review-api

## Scope

Verification of generic external references, append-only Review history,
revision counting, final commit SHA, JSON API v1 extensions, and SQLite
snapshot verification/restore.

## Review Results

- External references enforce task/project scope uniqueness, preserve removal
  history, and obey task lock ownership.
- Review records bind to immutable task versions, preserve reviewer attribution,
  and support stable idempotent retries. Only a new `changes_requested` record
  increments `revision_count`.
- Final commit assignment resolves a full Git SHA and obeys terminal/reopen
  constraints.
- `store.snapshot.verify` checks project identity, supported schema,
  migration validation, SQLite/foreign-key integrity, file size, and SHA-256.
- `store.snapshot.restore` locks and verifies the exact source snapshot,
  acquires the active database's exclusive lock before checking active task
  locks, captures the exact pre-restore state as a verified safety snapshot,
  and holds the exclusive lock through restore/rollback.
- Restore uses SQLite Online Backup API into the existing database connection;
  it does not replace the active database file. A checksum mismatch returns
  `snapshot_checksum_mismatch` before mutation.
- Restore rejects active task locks and retains the safety snapshot after
  success or rollback failure. It restores Todoing SQLite only; Git and
  consumer-owned OpenSpec artifacts are outside its scope.

## CI Evidence

- Full Linux/Windows Todoing CI passed on
  [45454426270](https://github.com/zhihus/todoing-x/actions/runs/45454426270):
  rustfmt, Clippy, full Cargo tests, release builds, install/smoke tests,
  SQLite lock spike, OpenSpec validation, and JSON Schema validation.
- The full CI workflow including the archive dependency gate passed on
  [45455174063](https://github.com/zhihus/todoing-x/actions/runs/45455174063).
- Real Manifest/OpenSpec acceptance passed on
  [45455395600](https://github.com/zhihus/Manifest/actions/runs/45455395600),
  building this Todoing feature-branch source at
  `6acca6087fbd4b39ad0dcd5f12db52dc4d84b163`.
- Acceptance exercised bootstrap, external references, approval baseline,
  lock custody, Review/idempotency/revision_count, rework, final SHA,
  completion, cancellation, OpenSpec archive modes, snapshot verification,
  restore, and post-restore data checks.

## Limitations

The cross-repository acceptance test runs on Linux in an isolated temporary
project. Windows coverage is provided by Todoing's full CI matrix. Recovery
from a database that cannot be opened by Todoing remains an offline/manual
procedure. No default branch was changed; no PR or merge was performed.

## Verdict

Implementation and verification are complete on the feature branch. Ready to
archive this completed OpenSpec change.
