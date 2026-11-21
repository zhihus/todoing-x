# Technical Research: Storage, Concurrency, and Configuration

**Status:** Research findings and recommendations; no final architecture decisions recorded.
**Date:** 2026-11-21
**Scope:** Core Todoing MVP defined by PRODUCT.md and the OpenSpec change core-task-capabilities.

## 1. Executive summary

The leading storage candidate is **SQLite through rusqlite with the bundled feature**. It is the best current fit for Todoing's relational task model, multi-record atomic transactions, dependency graph, append-only history, versions, and local multi-process access. It avoids introducing a database server and has a mature Rust integration. This is a recommendation for the next architecture stage, **not a final storage decision**.

**redb 5.x** is the strongest alternative. It offers ACID transactions, MVCC, crash safety, and documented multi-process modes including Windows. It is less attractive for this product because Todoing would need to implement and maintain relational indexing, constraints, joins, and graph traversal itself. Its 5.0 release also changes multi-process locking modes and compatibility requirements, so the exact pinned version and upgrade policy would need deliberate handling.

The most important unresolved issue is not raw database performance. It is the semantics of **exclusive task locks that also exclude reads by other clients for an entire agent work session**. Database transaction locks do not automatically provide this application-level guarantee. The selected storage engine and lock protocol must be tested together with multiple OS processes.

## 2. Requirements that drive the choice

The storage and concurrency design must support:

- Atomic commit of task state, a new immutable version, the corresponding operation-history entry, dependencies, and related metadata.
- Rollback with no partial task/version/history state after a failed operation.
- Multiple independent CLI processes accessing one project store.
- Stable, non-reused task identifiers.
- Relational integrity between tasks, dependencies, versions, history, statuses, and locks.
- Exclusive task locks that persist across separate CLI invocations and exclude non-owners from reads as well as writes.
- Recovery after the lock owner's process terminates unexpectedly.
- Windows and Unix-like platform support.
- A safe migration from Markdown/YAML task records and the JSON index.
- Local-first operation without a database server or network dependency.

Manifest currently runs agents sequentially, but Todoing remains an independent tool and must protect itself from concurrent clients.

## 3. Embedded storage candidates

| Candidate | Transaction and concurrency model | Data model | Portability / operational notes | Assessment |
|---|---|---|---|---|
| **SQLite + rusqlite** | ACID transactions; multiple readers; writes are serialized to one writer at a time. WAL allows readers and a writer to proceed concurrently. | Relational SQL, constraints, indexes, recursive CTEs. | Mature cross-platform engine; rusqlite can bundle SQLite to avoid relying on a system installation. WAL requires all processes on the same host and is not suitable for network filesystems. | **Preferred candidate.** Strong fit for task relationships and atomic multi-entity updates. |
| **redb 5.x** | ACID transactions, MVCC readers, crash safety, savepoints/rollback; documented multi-process SingleWriter and MultiWriter modes. | Embedded key-value store using copy-on-write B+ trees. | Pure Rust. Multi-process byte-range locking is documented for Linux, Apple platforms, and Windows. All processes must use compatible locking modes/protocol generations. | **Strong alternative.** Excellent Rust-native option, but relational features must be built in the application. |
| **LMDB via heed** | Multiple readers and a single writer transaction per environment. | Memory-mapped key-value database. | Requires careful map-size management; long-lived readers can prevent page reuse and grow the database. The Rust wrapper documents memory-map safety constraints and warns against remote filesystems. | Technically capable, but lower-level operational constraints add unnecessary design burden. |
| **RocksDB TransactionDB** | Pessimistic or optimistic transactions with conflict detection; atomic write batches. | LSM-tree key-value store. | Native RocksDB integration and tuning/compaction concerns add complexity. | Not justified by Todoing's expected local task workload. |
| **fjall** | Rust LSM engine; cross-partition snapshots and optional single-writer transactions. | Key-value partitions, not relational tables. | Pure Rust and embedded; background maintenance and LSM behavior must be operated and tested. | Viable technology, but offers no clear advantage over SQLite/redb for this data model. |
| **sled** | Multi-key and multi-tree ACID transactions are documented. | Key-value trees. | The current published crate line is 0.34.7; multi-process guarantees were not established by the reviewed documentation. | Do not shortlist without a separate maintenance and multi-process assessment. |

### 3.1 SQLite findings

SQLite provides serializable transactions by serializing writes: multiple connections can read, but only one writer can modify a database at a time. This is a useful and predictable model for a local task store, especially when the consuming Manifest workflow is sequential. A transaction ensures other connections see committed state rather than partially applied changes.

WAL mode improves reader/writer concurrency, but it still permits only one writer at a time. WAL depends on shared memory and is designed for processes on the same host; it must not be used to share a database over a network filesystem. SQLite also relies on correct filesystem locking, so network shares and filesystems with unreliable locks must be treated as unsupported storage locations.

The rusqlite crate provides a synchronous Rust API and a bundled feature that compiles a controlled SQLite version instead of depending on a machine-installed library. This is a practical distribution choice for Windows and other supported targets.

SQLite foreign-key constraints are disabled by default and must be enabled on every connection. Connection initialization therefore needs to apply and verify the required pragmas consistently. Recursive CTEs can traverse dependency graphs and support cycle-detection queries without maintaining a separate graph engine.

SQLite's transaction modes matter:
- BEGIN IMMEDIATE obtains the write transaction at the start, avoiding a later read-to-write upgrade race; it can return SQLITE_BUSY when another writer is active.
- BEGIN EXCLUSIVE has stronger reader-exclusion behavior in rollback-journal mode, but in WAL mode it behaves like IMMEDIATE for reader concurrency.
- The journal mode must therefore be selected together with the task-lock acquisition design, not by performance preference alone.

**Implication:** SQLite is a strong fit, but the architecture must define bounded busy handling, transaction retry rules, connection initialization, foreign-key enforcement, journal mode, and backup behavior. Do not expose arbitrary SQLite files or allow clients to bypass the Todoing storage layer.

### 3.2 redb findings

redb is a pure-Rust embedded key-value store with ACID transactions, MVCC, crash safety, savepoints, and documented multi-process access. Version 5.0 documents three concurrency modes:
- ExclusiveWriter: a writer excludes all other processes.
- SingleWriter: one writer process may coexist with read-only processes.
- MultiWriter: multiple processes may open the database for writing; write transactions are serialized.

The byte-range locking needed for the multi-process modes is documented for Linux, Apple platforms, and Windows. Every process sharing the database must use compatible concurrency modes and a compatible locking-protocol generation. The 5.0 release notes specifically describe changes to the multi-process locking protocol.

This is a credible alternative if avoiding a C dependency and using a Rust-native storage engine are given high priority. However, redb does not provide SQL, relational constraints, or joins. Todoing would have to implement secondary indexes, referential integrity, dependency traversal, and transactional query patterns itself. This increases application code and testing surface for features that map naturally to relational tables.

### 3.3 Other candidates

LMDB is mature and fast, but its memory-mapped design brings map sizing and reader-lifetime constraints. The heed documentation explicitly warns that long-lived read transactions can prevent page reuse and that remote filesystems are unsupported. It is not the simplest choice for a portable CLI whose primary concern is correctness rather than extreme throughput.

RocksDB supports transaction databases and atomic multi-key write batches. Its LSM design is optimized for high write throughput and large key-value workloads; compaction, tuning, and native integration are unnecessary complexity for the expected task volume.

fjall is a promising pure-Rust LSM option with snapshots and optional single-writer transactions, but it remains a key-value model. It does not remove the need to implement Todoing's relational integrity and query semantics.

sled documents ACID transactions across items and trees, but the reviewed published crate line is 0.34.7. The available material did not establish a sufficiently clear multi-process contract for this project, so it should not be selected without additional due diligence.

## 4. Preliminary storage recommendation

Proceed to an architecture spike with:

- **Candidate:** SQLite.
- **Rust access layer:** rusqlite, bundled feature.
- **Storage shape:** one database per project, with normalized relational tables.
- **Transaction boundary:** one database transaction for every task mutation, version append, operation-history append, dependency update, and related state.
- **Integrity:** foreign keys enabled on every connection; schema constraints for uniqueness and valid references.
- **Concurrency:** explicit transaction behavior, bounded busy timeout/retry, and multi-process tests.
- **Backup:** use a database-aware snapshot/backup mechanism; do not copy a live database file blindly.
- **Compatibility:** keep the current Markdown/YAML + JSON format as a migration/import-export concern, not as a second writable source of truth.

### 4.1 Cross-platform proof-of-concept results

The isolated SQLite spike was run by GitHub Actions on both ubuntu-latest and windows-latest. All five process-level tests passed on both platforms:

- An EXCLUSIVE transaction in rollback-journal mode waited for an already active reader to finish before the lock record was committed.
- After acquisition, a non-owner read was rejected while the lock owner retained access.
- Eight independent processes allocated unique task identifiers through serialized write transactions.
- An uncommitted transaction was rolled back after its process terminated without dropping the connection normally.
- A fencing generation rejected a former owner after an expired lock was recovered by another owner.

The existing application test suite, release build, isolated installation, and binary smoke test also passed on both operating systems after restoring the missing Clap argument types in main.rs. The initial CI run had exposed those missing type definitions; the correction restores the existing hooks command's argument model and does not change hook execution behavior.

**Updated recommendation:** use SQLite rollback-journal mode (DELETE) as the MVP starting point, with a short EXCLUSIVE transaction as the task-lock acquisition barrier. The prototype validates this mechanism on Windows and Linux. WAL is not recommended for this lock protocol because EXCLUSIVE does not provide the same reader barrier in WAL mode. The prototype is deliberately small: it does not prove production-scale behavior, power-loss durability, clock-change handling, or the final lease protocol. Those remain implementation tests.

## 5. Task-lock design findings

### 5.1 Database locks are not task locks

SQLite or redb transaction locks protect database consistency. They do not by themselves represent an agent's ownership of one task across multiple CLI calls. A task lock must survive the end of the todoing lock acquire process and remain valid while a separate agent performs work.

The application-level lock must therefore be represented as durable task state, or through a separate cross-process lock service. A database-backed lock record is the simplest candidate.

### 5.2 Required logical lock record

The design should evaluate a lock record containing:

- Task identifier.
- Owner/session identifier.
- Secret ownership token or equivalent capability.
- Lease expiry and last heartbeat timestamp.
- Monotonically increasing fencing generation/token.
- Acquisition and release/recovery metadata.

The fencing generation is important: if a lease expires and another client acquires the lock, a delayed command from the former owner must not be able to mutate the task using stale ownership. Every task read and mutation must pass through a lock-aware service and validate ownership in the same consistency boundary as the operation.

### 5.3 Read exclusion and acquisition race

A strict requirement that other clients cannot read a locked task needs more than checking a lock row in an unrelated query. The design must define the linearization point for lock acquisition:

1. Existing task operations already in progress must finish before the lock becomes active.
2. Once acquisition succeeds, new operations by non-owners must be rejected, including reads.
3. The lock owner must continue to read and mutate the task.
4. A stale owner must be rejected after expiry/recovery, even if it later resumes.

SQLite's rollback-journal EXCLUSIVE transaction can act as a short database-wide barrier against readers while acquisition commits; WAL has different reader behavior. This is a design option, not a selected implementation. A prototype must test the exact race across independent processes on Windows and Unix-like systems.

### 5.4 Lease and crash recovery

A persisted lease can survive process termination, but it needs a clock policy. A monotonic process-local timer cannot by itself represent a lease across independent CLI processes and restarts. A wall-clock expiry is straightforward but must define behavior for clock changes and machine sleep.

The architecture spike must test:
- Owner process termination and lease expiry.
- Heartbeat racing with recovery.
- Old owner resuming after a new owner acquired the task.
- System clock moving backwards or forwards.
- Two clients attempting recovery simultaneously.
- Database busy/lock errors during acquisition, renewal, and release.

OS file locks may help with process-level coordination and are released by the OS after a crash, but they do not by themselves provide a durable, cross-command agent-session lease.

## 6. Relational data model candidate

A normalized relational schema is a natural fit if SQLite is selected. The following are logical entities only, not an approved physical schema:

| Entity | Responsibility |
|---|---|
| metadata | Schema version and project-store metadata. |
| tasks | Stable task identity and current task state. |
| statuses | User-defined status definitions and enabled/disabled state. |
| status_transitions | Permitted transition rules. |
| task_dependencies | Directed task-to-task relationships with uniqueness and foreign-key constraints. |
| task_versions | Immutable snapshots for each successful task save. |
| operation_history | Append-only successful and rejected task-operation records. |
| task_locks | Current owner, lease, heartbeat, and fencing generation. |
| task_commit_refs | Git commit traceability metadata, if normalized separately. |

The schema must retain the current task fields (ID, title, creation timestamp, status, labels, body, and Git commit references) through migration. Whether labels and commit references are normalized or stored in structured columns remains a schema decision.

Dependency-cycle detection can use a recursive query or an application graph traversal inside the same transaction that validates and writes the relationship. The chosen approach must be tested for self-dependencies, direct cycles, indirect cycles, and concurrent opposing dependency changes.

## 7. Project configuration format

Status configuration is human-maintained project configuration, not the machine API. The candidates are:

| Format | Advantages | Risks / costs | Assessment |
|---|---|---|---|
| TOML | Comments, explicit types, readable tables/arrays, mature Serde integration, familiar in Rust projects. | Less suitable than JSON for direct machine interchange; needs a documented schema and validation. | **Preferred candidate for user-edited configuration.** |
| JSON | Strict and widely supported; easy to exchange with agents and tools. | No comments; less convenient for users editing workflow definitions. | Strong alternative if a single format for config and API data is prioritized. |
| YAML | Familiar from current task frontmatter and supports comments. | Implicit typing and parser-dialect differences; the current Rust port already contains compatibility code for Python/PyYAML behavior. | Do not carry YAML forward by inertia. |

Preliminary recommendation: TOML for project status configuration and JSON for the public machine interface. Keep both choices open until the configuration schema and API are designed. Do not use task Markdown frontmatter as the authoritative status configuration.

## 8. Backup and recovery implications

If SQLite is selected, a live database must be backed up through a database-aware method. SQLite's Online Backup API creates a consistent snapshot while allowing the source database to remain in use; rusqlite exposes this API behind its backup feature. A raw file copy while the database is active is not an adequate general backup contract, especially when journal/WAL files are involved.

Backup destination and retention remain the responsibility of Manifest/the consuming project, as already decided. Todoing's architecture should expose a safe snapshot or export mechanism that the consumer can invoke. Backups should be verified by reopening and integrity-checking the copied database before they are treated as valid.

## 9. Research recommendation and remaining decisions

| Decision area | Research position | Status |
|---|---|---|
| Primary database | SQLite is the preferred candidate; redb is the runner-up. | Prototype passed on Windows/Linux; final selection remains open pending approval. |
| Rust binding | rusqlite with bundled. | Cross-platform prototype passed; production integration remains to be validated. |
| Journal mode | Rollback-journal mode (DELETE) with a short EXCLUSIVE lock-acquisition barrier. | Preferred based on passing Windows/Linux prototype; final selection remains open. |
| Logical task locks | Durable database-backed lease + owner capability + fencing generation. | Recommended design direction; details open. |
| Lock barrier | Rollback-journal EXCLUSIVE transaction serializes acquisition against in-flight reads; later non-owner reads are rejected. | Validated in the Windows/Linux prototype; production integration tests remain. |
| Status config format | TOML is preferred; JSON is the alternative. | Preliminary. |
| Schema | Relational entities listed above. | Conceptual only; physical schema open. |
| Migration | Validated, non-destructive migration with source retained until verification. | Requirement accepted; procedure open. |
| Backup | Database-aware snapshot; Manifest owns destination/retention. | Requirement accepted; integration contract open. |

### Architecture spike status

- [x] Build a minimal multi-process SQLite prototype on Windows and Linux.
- [x] Test concurrent task creation and unique ID allocation.
- [x] Test a read racing with lock acquisition and prove the acquisition barrier.
- [x] Test lease expiry, recovery, fencing, and stale-owner rejection.
- [x] Test rollback after abrupt process termination with an uncommitted transaction.
- [ ] Test a database-aware backup snapshot while writes continue, then reopen and validate the snapshot.
- [ ] Compare the same lock and transaction tests against redb 5.x only if SQLite fails a required constraint or has unacceptable operational limitations.

## 10. Sources

- SQLite transactions: https://www.sqlite.org/lang_transaction.html
- SQLite isolation and single-writer behavior: https://www.sqlite.org/isolation.html
- SQLite WAL behavior and network-filesystem limitation: https://www.sqlite.org/wal.html
- SQLite atomic commit: https://www.sqlite.org/atomiccommit.html
- SQLite file-locking and corruption caveats: https://www.sqlite.org/howtocorrupt.html
- SQLite foreign-key enforcement: https://www.sqlite.org/foreignkeys.html
- SQLite recursive CTEs: https://www.sqlite.org/lang_with.html
- SQLite Online Backup API: https://www.sqlite.org/backup.html
- rusqlite and bundled SQLite: https://docs.rs/rusqlite/latest/rusqlite/
- rusqlite backup feature: https://docs.rs/rusqlite/latest/rusqlite/backup/
- redb crate capabilities: https://docs.rs/redb/latest/redb/
- redb 5.x multi-process locking design: https://github.com/cberner/redb/blob/master/docs/design.md
- heed/LMDB Rust wrapper: https://docs.rs/heed/latest/heed/
- RocksDB transaction model: https://github.com/facebook/rocksdb/wiki/Transactions
- fjall capabilities: https://docs.rs/fjall/latest/fjall/
- TOML Serde support: https://docs.rs/toml/latest/toml/
