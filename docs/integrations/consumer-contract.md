# External Consumer Integration Contract

**Status:** Current public integration contract for external orchestrators and project tools.
**Scope:** Generic consumer integration. Todoing remains standalone and contains no consumer-specific workflow policy.

## 1. Responsibility boundary

### Todoing provides

- One local SQLite task store per project.
- Configurable workflow statuses and permitted transitions.
- Task state, dependencies, immutable versions, append-only operation history, and approval baselines.
- Exclusive task locks with leases and fencing generations.
- CLI and versioned JSON API v1.
- Generic external references, append-only Review records, revision counters, and a distinct final Git commit marker.
- Ordinary Git commit references as task traceability metadata.
- Verified SQLite snapshots.

### The consuming workflow owns

- Stable project identity and project selection.
- Agent orchestration, role assignment, sequencing, and model selection.
- User approval and project-specific completion policy.
- OpenSpec workflow and Git branch/commit/review policy.
- Consistency checks across Todoing, OpenSpec, Git, and other connected components.
- Actor authentication and secure handling of lock tokens.
- Snapshot destination, retention, and replication.

Todoing does not parse OpenSpec, decide Git workflow, authenticate actors, or enforce a consumer's role policy. Integrations are optional; a consumer runs checks only for components it has connected and configured.

## 2. Project initialization and isolation

Each project has its own .todoing/todoing.db database. There is no central multi-project task database.

For a new project, a human can initialize the store with:

```bash
todoing init
todoing init --project-id my-project
```

The CLI generates a random project ID when the option is omitted. A consuming orchestrator should own a stable project ID and pass it explicitly.

The JSON API initializes a project through store.init:

```json
{
  "api_version": 1,
  "project_id": "my-project",
  "actor_id": "orchestrator",
  "operation": "store.init",
  "args": {}
}
```

Every subsequent API request MUST include the same project_id. Todoing compares it with database metadata and rejects a mismatch. The project ID is a namespace guard, not an authentication credential.

Do not call init over legacy Markdown task data. Todoing refuses this case and directs the caller to the non-destructive migration operation.

## 3. JSON API request model

API v1 processes one JSON request per todoing api process:

- Required envelope fields: api_version, project_id, actor_id, operation, args.
- Optional fields: request_id and lock_token.
- request_id is echoed unchanged when supplied.
- One JSON response is written to stdout. Diagnostics go to stderr.
- Errors contain a stable machine-readable code and are accompanied by a non-zero process exit code.

actor_id is attribution recorded in history; it is not authentication. A consuming orchestrator is responsible for its own trust and identity model.

The normative operation list and argument/result contracts are in ../api/v1/operations.md. Request and response schemas are in ../api/v1/.

## 4. Workflow status and outcomes

Status definitions and transitions are stored in .todoing/config.toml using TOML format version 1.

- Status IDs, display labels, ordering, enabled state, and permitted transitions are project-controlled.
- Todoing does not impose a consumer-specific status set and does not automatically synchronize status definitions.
- A consumer may provide a recommended configuration, but applying or changing it is an explicit project configuration action.
- A disabled status remains visible in historical task data but cannot be selected for new transitions.
- Task outcome (completed or cancelled) is distinct from workflow status. Explicit completion/cancellation operations can set an outcome without changing status.

Status mapping between a consumer's workflow and Todoing's configurable statuses is a consumer-side decision. Todoing must not infer a mapping from labels or status names.

## 5. Exclusive task locks

A consumer that requires exclusive ownership for an agent work session should:

1. Acquire a task lock.
2. Keep the returned owner ID, lock token, and fencing generation.
3. Supply lock_token in the JSON API request envelope for owner operations.
4. Renew the lease with heartbeat while work continues.
5. Release the lock at the end of the session.

The raw token is returned once and only its SHA-256 hash is stored by Todoing. Consumers must keep it secret and must not place it in task content, logs, command-line arguments, or Git. For CLI owner operations, the token is read from TODOING_LOCK_TOKEN.

While a task is locked, task.get and task-scoped reads/writes (including history, versions, external references, and Review records) require the valid owner token; non-owner requests are rejected without disclosing protected data. task.list/search omit locked tasks. Unlocked tasks may be read or changed without a token. Recovery is explicit; an expired lock does not silently grant ownership. Fencing generations prevent a former owner from resuming after recovery.

Todoing's local actor ID is attribution only. The consumer remains responsible for protecting credentials and deciding which process may act as an owner.

## 6. External references, Review history, Git, and snapshots

`task.external-ref.add/list/remove` store generic typed references such as OpenSpec Change IDs or issue identifiers. Each reference has a namespace, value, relation, and task/project scope. Active uniqueness is enforced within the selected scope; removal marks a reference inactive and preserves its history. Identical retries are no-ops.

`task.review.add/list` persist append-only Review records linked to immutable task versions. A Review ID is unique within a task; an identical retry is a no-op and a conflicting payload is rejected. Each newly accepted `changes_requested` record increments `revision_count` exactly once; `approved` does not. Todoing does not decide whether a review outcome satisfies a consumer's approval policy.

`task.commit.add` stores ordinary Git commit references as traceability metadata. `task.commit.finalize` resolves a supplied Git revision to a full commit SHA and stores one explicit final commit marker. Repeating the same SHA is a no-op; replacing it requires reopening the task after the prior assignment. A linked or final commit is not proof of completion, and Todoing does not enforce branch, commit, review, or merge policy.

store.snapshot creates and verifies a consistent SQLite snapshot. Snapshot creation is rejected while any task lock is active. The consumer chooses the destination and owns retention, replication, and backup scheduling. Copying the live database file is not a substitute for the snapshot operation.

## 7. Events, hooks, and cross-component checks

The SQLite backend does not execute the legacy Markdown completion hooks. A generic lifecycle event API is deferred; no delivery, ordering, retry, or failure semantics are currently promised.

A consumer must not depend on Todoing hooks for correctness. It may run its own configured checks before or after Todoing operations. Cross-component consistency among tasks, OpenSpec artifacts, Git branches/commits, and other connected systems belongs to the consumer, not to Todoing.

## 8. Explicitly unresolved consumer decisions

The following are not inferred by Todoing and must be resolved by each consuming workflow where relevant:

- Mapping between consumer workflow states and Todoing status IDs.
- Actor authentication and trust boundaries.
- Secure lock-token custody across processes and sessions.
- Any relationship between Todoing task IDs and external specification/work-item identifiers.
- Event delivery semantics, if a concrete use case later justifies a generic event mechanism.
- Backup destination, retention, and replication policy.

No consumer-specific behavior should be added to Todoing core to resolve these questions.
