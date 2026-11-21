# Design Proposals: History, JSON Contract, and Migration

**Status:** Recommended design options; not final decisions  
**Date:** 21 November 2026  
**Authority:** Companion to `openspec/changes/core-task-capabilities/design.md`. The OpenSpec change and accepted product decisions remain normative.

## 1. Purpose

The core capability design leaves several implementation-facing questions open. This document narrows the options for:

- Actor identity and operation-history records.
- Immutable task-version snapshots and version comparison.
- The versioned CLI/JSON contract and error mapping.
- Migration from Markdown/YAML task records and the JSON index.

Recommendations here are proposals for review, not silent changes to accepted decisions.

## 2. Actor identity and operation history

### 2.1 Actor identity

A display name alone is not a reliable actor identity. Names can change or collide, and Todoing must not depend on a specific agent harness or identity provider.

**Recommended logical actor fields:**

| Field | Purpose |
|---|---|
| `actor_id` | Stable identifier supplied by the client or its local configuration |
| `actor_type` | Generic category such as `human`, `agent`, or `system` |
| `actor_label` | Optional display name captured for readable history |

The actor type is descriptive metadata, not an authorization policy. Todoing should not embed provider-specific agent names, Manifest roles, or external account credentials.

Open decisions:
- Whether actor identity is mandatory for every mutation or whether the CLI can resolve a configured local default.
- Whether an actor ID is project-local or globally stable.
- How anonymous/manual CLI operations are represented.
- Whether actor identity is supplied on each command, through a session/lock token, or through project configuration.

Do not store access tokens, passwords, or other secrets in operation history.

### 2.2 Operation-history record

A history entry should be an immutable record with:

- Event ID and, where applicable, task ID.
- Actor identity snapshot.
- UTC timestamp.
- Stable operation identifier.
- Previous and requested/new values or structured payloads.
- Result category: committed, rejected, or another explicitly defined outcome.
- Optional diagnostic details that do not contain secrets.

The operation identifier should be a stable machine value, not a localized display string. Human-readable descriptions should be derived by clients.

### 2.3 Rejected operations

The accepted OpenSpec design requires rejected task-operation attempts that reach the task-operation layer to be recorded. A rejected attempt:

- Adds a history record with the rejection result.
- Does not create a successful task version.
- Does not change committed task state.

A failure that prevents the history write cannot be recorded in that same store. Todoing must return a storage error and must not claim that the operation was audited.

One unresolved edge case is a rejected task-creation attempt: no task ID exists yet, while the current conceptual history model is task-scoped. Options are:
1. Permit project-level history entries with a nullable task ID and a request/attempt ID.
2. Assign a non-reusable task ID before validation and reserve it even if creation is rejected.

**Recommendation:** use project-level operation history for attempts that do not yet have a task ID. Do not create a task record or consume a permanent task ID merely to record an invalid create request.

### 2.4 Atomicity boundary

For a successful task mutation, the task row, new version, successful history entry, and required related state must commit in one transaction.

For a business-rule rejection, no task mutation or version is committed; the rejection history entry is a separate atomic history-only operation. This distinction avoids representing a rejected operation as a successful task save.

The exact handling of infrastructure failures during a history-only write remains an operational error case, not a reason to weaken task-state atomicity.

## 3. Task versions and comparison

### 3.1 Snapshot contents

A version should be a complete canonical snapshot of the task at the moment of a successful save, including:

- Stable task ID.
- Title and description/body.
- Workflow status and completion/cancellation outcome.
- Labels.
- Git commit references.
- Dependencies.
- Structured metadata supported by the task model.

Creation/update timestamps and version metadata should be captured consistently. Actor and version creation time belong to the version record even if they are also included in the snapshot.

Snapshots are immutable. A later task update creates a new version; it does not edit an earlier snapshot.

### 3.2 Version numbering and baseline

- Version numbers are monotonically increasing within a task.
- Every successful task save that changes task data creates exactly one new version.
- A no-op save does not create a redundant version.
- The user-approved baseline references one existing immutable version.
- Re-approval moves the current baseline reference; it does not alter prior versions or erase approval history.

Whether approval itself is a task version or only a history event should remain explicit. The current product requirement is that the baseline version be identifiable; approval metadata does not need to rewrite the task snapshot.

### 3.3 Version comparison

**Recommendation:** store canonical snapshots and calculate structured differences on request rather than persisting a separate diff for every version pair.

A comparison should:
- Require two versions belonging to the same task.
- Identify changed fields.
- Represent additions/removals in collections such as labels, dependencies, and commit references.
- Preserve the original values and never mutate snapshots.
- Return a deterministic structure suitable for both human and JSON clients.

The exact diff schema, treatment of reordered collections, and text-body diff format remain open API design decisions. A text patch may be an optional presentation layer; it should not replace the structured field comparison.

## 4. Versioned CLI/JSON contract

### 4.1 Contract boundary

The CLI remains the primary interface. Human-readable output and machine-readable JSON must call the same application services.

JSON is a public contract, not an incidental serialization of internal Rust structs. Internal database models should not be serialized directly as the long-term API.

**Recommended response envelope:**

Success:
```json
{
  "api_version": "1",
  "ok": true,
  "data": {}
}
```

Error:
```json
{
  "api_version": "1",
  "ok": false,
  "error": {
    "code": "task_not_found",
    "message": "Task was not found",
    "details": {}
  }
}
```

These are illustrative shapes, not a final schema.

### 4.2 Versioning

- Every machine response identifies the API version.
- A client may request a supported version explicitly; the exact CLI flag/request field is open.
- Additive optional fields may be introduced within a version if existing clients remain compatible.
- Removing fields, changing field meaning, or changing error semantics requires a new API version.
- Rust type names, SQL column names, and internal enum serialization must not define the public contract accidentally.

### 4.3 Error model

Errors should have stable machine codes independent of their human-readable messages.

The catalogue should cover at least:
- Invalid arguments/configuration.
- Task not found.
- Invalid status or prohibited transition.
- Dependency conflict/cycle.
- Lock conflict or stale ownership.
- Version/baseline not found.
- Migration conflict or invalid source data.
- Storage busy, integrity, and I/O failures.
- Internal unexpected failure.

The process exit code should identify a stable broad category; the JSON error code provides precise machine handling. Exact numeric values and retryability metadata remain open.

For JSON invocations:
- stdout contains exactly one JSON response.
- Human progress, warnings, and diagnostics go to stderr.
- A failed operation still returns a valid error envelope whenever the process can serialize one.
- A process-level failure that prevents JSON serialization is reported through stderr and a non-zero exit code.

### 4.4 Compatibility

The current `--json` option is a compatibility starting point, not a versioned API. The transition must be explicit:
- Preserve existing CLI behavior where practical.
- Document any changed command or output semantics.
- Add contract tests for every supported API version.
- Test that errors and diagnostics do not contaminate stdout.

## 5. Legacy migration

### 5.1 Source-of-truth rules

The current store contains Markdown task files with YAML frontmatter and a rebuildable JSON index.

The Markdown task files are the recoverable task records; the index is derived and can be rebuilt. Migration should nevertheless inspect both sources and report inconsistencies rather than silently ignore them.

Recommended handling:
- Valid Markdown task records are the primary import source.
- A stale or missing index is reported and can be reconstructed from Markdown.
- An index entry with no corresponding Markdown file is reported as an orphan; it is not silently imported as a complete task.
- Conflicting fields are reported. Where the existing implementation defines Markdown as authoritative, use that value only after recording the discrepancy.
- Malformed Markdown/frontmatter is reported with its file path and reason; migration must not silently skip it.

### 5.2 Preservation

Migration must preserve, where representable:
- Task IDs and their non-reuse guarantee.
- Titles, creation timestamps, statuses, labels, and description bodies.
- Git commit references.
- Supported metadata and unknown legacy frontmatter that cannot yet be mapped to a typed field.

Unknown fields should be retained in a compatibility payload or reported for explicit resolution. Do not discard them silently.

### 5.3 Staged migration

**Recommended sequence:**

1. Inspect the source store without modifying it.
2. Validate task files and compare them with the JSON index.
3. Produce a migration report containing valid records, conflicts, malformed files, and unsupported fields.
4. Create the target database separately from the source files.
5. Import all accepted records in transactions.
6. Validate counts, IDs, references, dependencies, metadata, and database integrity.
7. Activate the target store only after validation succeeds.
8. Keep the original Markdown/YAML files and JSON index unchanged as a recovery source until the user/framework confirms the migrated store is usable.

A failed migration must leave the original store available. A partially populated target database must not be mistaken for an activated store.

### 5.4 Repeatability and rollback

The design must define how to handle:
- A second migration attempt when a target database already exists.
- Interrupted migration and restart.
- A target database created by an earlier failed attempt.
- Conflicting task IDs or duplicate source records.
- A source store modified between inspection and cutover.

**Recommendation:** migration should be explicit and guarded by a migration-state marker. Do not overwrite an existing target database automatically. If the source changes after inspection, abort and require a fresh validation pass.

A dry-run/report-only mode is useful but is not yet an accepted requirement. Decide whether it belongs in the MVP before adding it to the public CLI contract.

## 6. Required acceptance tests

### History and versions
- Successful mutation creates exactly one version and one successful history event.
- No-op save creates neither.
- Rejected status/dependency/lock operation creates a rejection event but no task version or state change.
- Rejected create attempt can be recorded without inventing a task ID.
- Version snapshots remain byte/logically unchanged after later updates.
- Baseline can move to another existing version without changing snapshots.
- Version comparison is deterministic and read-only.

### JSON contract
- Every supported command returns the documented versioned envelope.
- Success and failure responses are valid JSON.
- stdout contains no diagnostics or progress text.
- Error codes and exit codes remain stable across supported platforms.
- Contract tests detect incompatible changes.

### Migration
- Valid legacy fixture migrates with all supported fields and IDs preserved.
- Missing/stale index is detected and reconstructed or reported.
- Orphan index entries and malformed Markdown are reported.
- Unknown frontmatter is retained or explicitly reported.
- Failure at each migration stage leaves source files unchanged.
- Re-running migration does not overwrite an existing target store.
- Restored target database passes integrity and foreign-key checks.

## 7. Decisions still open

1. Actor identity fields, source, scope, and behavior for manual CLI operations.
2. Whether rejected create attempts are included in project-level history and the exact event schema.
3. Version snapshot schema/versioning and structured diff representation.
4. Approval history semantics and whether approval requires a separate actor identity.
5. JSON API version negotiation, exact envelopes, error codes, and exit-code values.
6. Whether all commands support JSON in MVP or only the documented machine operations.
7. Conflict policy for orphan index entries and Markdown/index field mismatches.
8. Migration-state marker, restart behavior, cutover, and rollback mechanics.
9. Whether migration dry-run/report-only mode is part of MVP.

No implementation should silently choose these items. Record accepted decisions in the architecture decision register and update the OpenSpec design before implementation.
