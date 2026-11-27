# External Consumer Integration Contract

**Scope:** Generic contract for external orchestrators. Todoing stays consumer-agnostic.

## 1. Responsibility
**Todoing:** per-project SQLite store; configurable statuses/transitions; tasks, dependencies, immutable versions and append-only history; lease/fencing locks; CLI and JSON API v1; generic external refs, append-only Reviews, revision counters, final Git commit marker; verified snapshots.

**Consumer:** project identity/selection; agent roles, sequencing and model; user approval and completion policy; OpenSpec/Git/review rules; cross-system checks; actor authentication and lock-token custody; backup destination/retention/replication.

Todoing does not parse OpenSpec, authenticate actors or enforce consumer workflow/role policy. Integrations and checks are optional and consumer-configured.

## 2. Project isolation and bootstrap
Each project has its own `.todoing/todoing.db`; no central multi-project store.
- CLI: `todoing init [--project-id ID]`. CLI generates an ID if omitted; orchestrators should supply a stable ID.
- API: `store.init` with `api_version=1`, `project_id`, `actor_id`, `operation`, `args`.
- Every request must use the same project ID; mismatch with DB metadata is rejected. ID is a namespace guard, not authentication.
- Never initialize over legacy Markdown task data; use non-destructive migration.

## 3. JSON API v1
One request per `todoing api` process. Required envelope: `api_version`, `project_id`, `actor_id`, `operation`, `args`; optional: `request_id`, `lock_token`.
One JSON response goes to stdout; diagnostics to stderr. Supplied `request_id` is echoed. Errors have stable codes and non-zero exit codes.

`actor_id` is history attribution, not authentication. The consumer owns identity/trust. Normative operations and schemas: `docs/api/v1/` and `../api/v1/operations.md`.

## 4. Statuses and outcomes
Definitions/transitions live in `.todoing/config.toml` (TOML v1). The project controls IDs, labels, order, enabled state and transitions; Todoing imposes no consumer-specific status set or automatic sync.
Disabled statuses remain in history but cannot be newly selected. Outcome (`completed|cancelled`) is distinct from status. Consumer owns status mapping; Todoing must not infer it from names/labels.

## 5. Exclusive locks
For an exclusive agent session: acquire → retain owner ID/token/fencing generation → pass `lock_token` in API envelope → heartbeat → release.
- Token is returned once; only its SHA-256 is stored. Keep it secret; never put it in task content, logs, CLI args or Git. CLI reads `TODOING_LOCK_TOKEN`.
- While locked, task-scoped reads/writes (including history, versions, refs and Reviews) require the owner token; non-owners are rejected without protected data. `task.list/search` omit locked tasks.
- Expiry does not transfer ownership automatically. Recovery is explicit; fencing blocks former owners.
- Actor ID is attribution only. Consumer protects credentials and controls which process may act as owner.

## 6. References, Reviews, Git and snapshots
- **External refs:** `task.external-ref.add/list/remove`; typed namespace/value/relation with task or project scope. Active uniqueness applies within scope. Removal deactivates but preserves history; identical retries are no-ops.
- **Reviews:** `task.review.add/list`; append-only, linked to immutable task versions. Review ID is unique per task; identical retry is a no-op, conflicting payload rejected. Each accepted `changes_requested` increments `revision_count` once; `approved` does not. Consumer defines approval policy.
- **Git:** `task.commit.add` stores traceability refs. `task.commit.finalize` resolves a revision to full SHA and records one final marker. Same SHA retry is a no-op; replacement requires reopening the task. A commit is not proof of completion; branch/review/merge policy belongs to consumer.
- **Snapshots:** `store.snapshot` creates a consistent SQLite backup. `store.snapshot.verify` checks project identity, supported schema, SQLite/FK integrity, size and SHA-256; mismatch returns `snapshot_checksum_mismatch`. `store.snapshot.restore` restores a verified same-project snapshot via SQLite Online Backup API; digest mismatch fails before mutation. Create/restore are rejected while any task lock is active. Restore takes an exclusive DB lock, retains a verified pre-restore backup in `.todoing/backups/`, and rolls back from it if verification fails. Restore affects Todoing SQLite only, not Git/OpenSpec/consumer artifacts. Consumer owns destination and retention. Never copy the live DB as a substitute.

## 7. Hooks and cross-system checks
SQLite does not run legacy Markdown completion hooks. No generic lifecycle event delivery, ordering, retry or failure guarantees exist. Consumers must not rely on Todoing hooks; their own configured checks and consistency across Todoing, OpenSpec, Git and other systems remain consumer-owned.

## 8. Consumer decisions
Where relevant, each consumer must define: status mapping; actor authentication/trust; secure lock-token custody; task-ID/external-ID relationship; any future event requirements; backup destination/retention/replication. Do not add consumer-specific policy to Todoing core.

## Private data
The entire `.todoing/` directory is private local data and must never be committed or synchronized: DB, config, legacy tasks, migration reports and snapshots. Todoing adds `/.todoing/` to Git ignore and rejects `init/migrate` if any path below it is tracked. Never force-add it; keep backups outside Git repositories.
