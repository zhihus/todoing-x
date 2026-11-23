## 1. Requirements and contract

- [ ] 1.1 Add delta requirements for generic external references, including uniqueness, lock enforcement, removal history, and version snapshots.
- [ ] 1.2 Add delta requirements for append-only Review records, reviewed-version binding, reviewer attribution, and idempotency.
- [ ] 1.3 Add delta requirements for persisted revision_count and exact increment semantics.
- [ ] 1.4 Add delta requirements for one explicit final commit marker and conflict behavior.
- [ ] 1.5 Extend JSON API v1 operation, request, response, and error-code requirements.

## 2. Tests first

- [ ] 2.1 Add storage tests for external-reference add/list/remove, uniqueness, no-op, and lock exclusion.
- [ ] 2.2 Add storage tests for Review records, version binding, idempotency, and conflicting review_id payloads.
- [ ] 2.3 Add tests proving revision_count increments once per changes_requested record and never for approved/no-op.
- [ ] 2.4 Add tests for final commit resolution, repeated same SHA, and conflicting replacement.
- [ ] 2.5 Add migration tests from schema v2 preserving tasks, versions, history, locks, dependencies, and archive metadata.
- [ ] 2.6 Add API contract tests for every new operation and stable error response.

## 3. Storage and domain implementation

- [ ] 3.1 Add migration 0003 with external-reference and Review tables, task metadata columns, indexes, and constraints.
- [ ] 3.2 Add domain types and validation for external references and Review records.
- [ ] 3.3 Extend task loading, snapshots, versions, diffs, and store verification for the new metadata.

## 4. Repository operations

- [ ] 4.1 Implement external-reference add/list/remove with lock enforcement and atomic history/version writes.
- [ ] 4.2 Implement Review add/list with exact idempotency, version binding, and atomic revision_count update.
- [ ] 4.3 Implement final commit marker assignment with Git SHA resolution and conflict handling.

## 5. JSON API and documentation

- [ ] 5.1 Add API dispatch, argument types, response fields, and error mapping for the new operations.
- [ ] 5.2 Update request/response schemas and the normative operations document.
- [ ] 5.3 Update consumer contract and current-behavior OpenSpec specs after implementation passes tests.

## 6. Verification

- [ ] 6.1 Validate OpenSpec and JSON schemas.
- [ ] 6.2 Run focused tests, full cargo test, and cargo build --release.
- [ ] 6.3 Verify Windows and Linux CI results.
- [ ] 6.4 Perform an independent code review and reconcile all findings before merge.
