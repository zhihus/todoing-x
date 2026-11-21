# Storage Engine Research

**Date:** 21 November 2026  
**Status:** Technical evaluation; no final storage decision  
**Scope:** Embedded, project-local storage for the Todoing MVP

## 1. Requirements that drive the choice

The storage engine must support:

- Atomic commits spanning task state, version snapshots, operation history, dependencies, lock state, and related indexes.
- Multiple local CLI processes without silent overwrites or duplicate task IDs.
- Exclusive task-work locks that survive process termination and expire through a defined lease protocol.
- Efficient task, dependency, history, and version queries.
- Append-only history and immutable versions, with potentially unbounded growth.
- Windows and Unix-like systems.
- Project-local operation without a database server or Git dependency.
- A safe backup/snapshot path that can write to a separate disk or remote destination.
- Migration from the existing Markdown/YAML task files and JSON index.

A database transaction is not itself the long-running task lock. Agent work may last minutes or hours; holding a database transaction for that duration would be inappropriate. The lock must be durable state with a lease, while each individual operation uses a short transaction to check ownership and perform its work.

## 2. Candidates

| Candidate | Transaction model | Concurrency | Relational model | Assessment |
|---|---|---|---|---|
| SQLite via `rusqlite` | ACID transactions | Multiple readers; one writer at a time; WAL permits reader/writer overlap | Native SQL, indexes, constraints, joins | **Primary candidate** |
| redb 5.x | ACID transactions, crash-safe | Concurrent readers and writer; configurable multi-process modes; one active writer transaction at a time | Key/value tables and B-trees; relationships and indexes are application-managed | **Strong alternative** |
| LMDB via `heed` | ACID transactions | Multiple readers; serialized writers | Key/value; indexes and relations are application-managed | Secondary alternative |
| sled | Multi-key/keyspace transactions | Current documented model does not support multiple open database instances | Key/value; application-managed | Do not select for this workload |
| Markdown/YAML + JSON files | Atomic replacement of individual files only | Application must coordinate all processes and multi-file changes | No native relational constraints | Not suitable as the transactional primary store |

No performance benchmark has been run. Absolute throughput or latency comparisons would be speculative.

## 3. SQLite with rusqlite

### Verified characteristics

- SQLite provides atomic transactions: a transaction is committed as a whole or rolled back as a whole.
- SQLite serializes writes; only one writer can write to a database at a time.
- WAL mode allows readers and a writer to proceed concurrently, while writes remain serialized.
- WAL requires all database processes to run on the same host and does not work over a network filesystem.
- In WAL mode, `synchronous=FULL` provides ACID durability; `NORMAL` preserves consistency but a recently committed transaction can be lost after power failure or system crash.
- Foreign-key enforcement is disabled by default and must be enabled on each connection.
- `rusqlite` exposes transactions and an optional online backup API. Its `bundled` feature builds SQLite with the application and avoids depending on the system SQLite installation.

### Fit for Todoing

**Advantages**
- The task model is naturally relational: tasks, dependencies, versions, history entries, status definitions, and locks have explicit relationships.
- Foreign keys, uniqueness constraints, and indexes can enforce many invariants at the storage boundary.
- One transaction can update a task, append a version and history record, update derived state, and commit them together.
- SQL supports version/history inspection and dependency queries without building a custom query/index layer.
- The online backup API can create a consistent snapshot while the source database remains active.
- `rusqlite` is a direct Rust integration and `bundled` reduces deployment differences across Windows and Unix-like targets.

**Constraints**
- One writer at a time. This is a serialization point, but it is consistent with Manifest's sequential agent workflow. Concurrent clients still require contention tests and predictable busy/timeout handling.
- WAL is unsuitable when the project database is placed on a network filesystem. The supported storage-location policy must be explicit.
- A database transaction does not implement Todoing's long-lived exclusive task lock. That lock must be represented separately and checked by every task operation, including reads.
- Foreign-key enforcement must be explicitly enabled on every connection; relying on SQLite defaults would leave referential integrity unenforced.
- WAL checkpointing and backup behavior must be part of operational design. Copying only the main database file while WAL is active is not an adequate backup procedure.

### Candidate configuration for a prototype

Not a final architecture decision:

- `rusqlite` with `bundled`.
- WAL journal mode for local filesystems.
- `synchronous=FULL` as the initial durability baseline.
- `foreign_keys=ON` on every opened connection.
- Short-lived transactions; never hold a SQL transaction for the duration of an agent's work session.
- Explicit busy timeout/retry behavior.
- Online backup API or a verified equivalent for consistent snapshots.

The final durability and journal-mode settings require crash testing on supported operating systems and filesystems.

## 4. redb

### Verified characteristics

- redb is a pure-Rust embedded key/value database using copy-on-write B+trees.
- The project documents ACID transactions, crash safety, savepoints, and concurrent readers with a writer.
- redb 5.0.0 was published on 18 November 2026 and is marked stable; it documents multi-process access with exclusive-writer, single-writer, and multi-writer concurrency modes.
- In single-writer mode, one process writes while other processes may open read-only handles. Multi-writer mode coordinates multiple read/write processes, but has additional restrictions.
- Platforms without file-range locks support only the exclusive-writer mode.

### Fit for Todoing

**Advantages**
- Pure Rust implementation reduces native C dependency and may simplify cross-platform builds.
- Transactional key/value operations can atomically update several records and indexes.
- Concurrent readers and a serialized writer are compatible with the expected workload.
- The API exposes integrity checking and compaction.

**Constraints**
- There is no SQL layer. Dependency traversal, history queries, version comparison, unique constraints, and secondary indexes must be designed and maintained by Todoing.
- The multi-process concurrency mode and platform-dependent file-locking behavior need explicit Windows and Unix validation.
- Version 5.0 is very recent. Its file format is described as stable, but the new major-version and multi-process behavior should be exercised in a prototype before adoption.
- A safe online backup/snapshot procedure was not established from the reviewed documentation. Backup and restore must be demonstrated, not assumed from crash-safety claims.
- Manual secondary-index maintenance increases the number of invariants the application must test.

### Assessment

redb is a credible alternative if avoiding native SQLite and retaining a Rust-only storage stack is a high priority. It is less attractive if relational constraints and rich history/dependency queries are central, because those become application responsibilities.

## 5. LMDB via heed

### Verified characteristics

- `heed` is a Rust wrapper around LMDB.
- LMDB allows multiple concurrent read transactions but only one read-write transaction at a time.
- Long-lived read transactions prevent reuse of pages freed by newer writes and can cause database growth.
- The `heed` documentation records a macOS/POSIX semaphore limitation and warns about the effects of process crashes in that locking section.
- LMDB relies on memory mapping and file locking; its documentation advises against remote filesystems.

### Fit for Todoing

LMDB is a mature embedded key/value design and can provide strong transactional behavior. However, it has the same application-managed relationship/index burden as redb, while adding map-size management and platform-specific operational concerns. The documented long-lived-reader behavior is also relevant to version/history inspection if readers are not carefully bounded.

**Assessment:** retain as a reference point, but do not prioritize it over SQLite or redb for the first prototype.

## 6. sled

The current repository README explicitly says it is out of sync with a large in-progress rewrite. It also states that multiple open database instances are not supported in the documented model, describes sled as beta, and advises using SQLite when reliability is the primary constraint. The published `0.34.7` release dates from September 2021.

Todoing is a multi-process CLI: separate invocations must safely access the same project store. The current documented sled model does not meet that requirement. The in-progress rewrite is not a stable dependency target.

**Assessment:** exclude sled from the MVP shortlist unless a later stable release documents and demonstrates the required multi-process guarantees.

## 7. Existing Markdown/YAML and JSON files

The current format is human-readable and easy to version in Git. It is useful as an import/export representation.

It is not a suitable transactional primary store for the target requirements:

- Atomic replacement of one file does not make a task file, version record, history entry, and index one atomic operation.
- Cross-process coordination, lock recovery, ID allocation, and dependency integrity would all need a separate transaction protocol.
- The current implementation already permits a task file to be updated while the index remains stale if the second write fails.
- As history and versions grow, listing and comparison would require custom indexes and scanning/maintenance logic.

**Assessment:** preserve it as a migration source and potentially as a human-readable interchange format, not as the authoritative transactional store.

## 8. Preliminary recommendation

**Use SQLite via rusqlite as the first implementation candidate for a focused prototype. Do not record it as the final selection yet.**

Reasons:

1. The required data model is relational and benefits from foreign keys, uniqueness constraints, and indexed joins.
2. Multi-entity atomic commits map directly to SQLite transactions.
3. One-writer serialization is acceptable for the current sequential-agent workflow, subject to contention tests.
4. The Rust binding offers a bundled build and an online backup API.
5. The operational and recovery model is well documented and substantially simpler than implementing relational invariants over a key/value engine.

Keep redb as the comparison prototype. It may win if the pure-Rust dependency model and multi-process behavior prove materially better for the required workload, and its backup/restore story passes the same tests.

This is a recommendation for prototype order, not approval to implement SQLite or change the product decision register.

## 9. Required prototype and validation work

Before a final storage decision, implement comparable throwaway prototypes for SQLite and redb against the same workload and acceptance tests:

1. Atomic task + version + history + dependency + lock-state commit.
2. Injected failure at each write stage; verify full rollback and previous-state visibility.
3. Two or more independent Todoing processes performing concurrent reads, writes, ID allocation, and lock acquisition.
4. Lock acquisition/renewal/release, stale lease recovery, and process termination.
5. Process kill and simulated crash around commit boundaries; verify integrity and committed-state durability.
6. Windows and Linux CI, including concurrent-process tests.
7. Backup while writes are active, restore into a separate directory, and integrity verification.
8. Large history/version datasets: append, query, compare, and compact/maintain without losing immutable records.
9. Measure representative read/write latency, database growth, backup time, restore time, and build impact. Define acceptance thresholds before running the benchmark.

Also resolve the database-location policy. SQLite WAL assumes a local filesystem on one host; a project on a network share must either be explicitly unsupported for concurrent access or use a separately evaluated mode.

## 10. Architecture questions that remain open

- Whether project configuration (including statuses) is stored in the database or in a separate file, and how configuration changes are made consistent with task mutations.
- Exact task-lock ownership token, lease duration, heartbeat cadence, and expiry/recovery semantics.
- How read operations linearize against concurrent lock acquisition.
- Whether lock operations themselves create history entries and task versions.
- Version snapshot representation and diff semantics.
- Schema migration/versioning and rollback procedure.
- Backup ownership remains with Manifest/consuming project, but Todoing must expose a safe, consistent snapshot mechanism.
- Whether support for network-mounted project directories is required.

## 11. Sources

### SQLite
- [SQLite transactions](https://www.sqlite.org/lang_transaction.html)
- [SQLite isolation and write serialization](https://www.sqlite.org/isolation.html)
- [SQLite WAL: concurrency and same-host restriction](https://www.sqlite.org/wal.html)
- [SQLite synchronous and durability settings](https://www.sqlite.org/pragma.html)
- [SQLite foreign-key enforcement](https://www.sqlite.org/foreignkeys.html)
- [SQLite online backup API](https://sqlite.org/backup.html)
- [rusqlite documentation](https://docs.rs/rusqlite/latest/rusqlite/)
- [rusqlite bundled feature guidance](https://github.com/rusqlite/rusqlite)

### redb
- [redb 5.0 documentation](https://docs.rs/redb/latest/redb/)
- [redb 5.0.0 release metadata](https://docs.rs/crate/redb/latest)
- [redb multi-process concurrency design](https://github.com/cberner/redb/blob/master/docs/design.md)

### LMDB / heed
- [heed documentation](https://docs.rs/heed/latest/heed/)
- [heed environment transactions and limitations](https://docs.rs/heed/latest/heed/struct.Env.html)

### sled
- [Current sled repository README](https://github.com/spacejam/sled)
- [sled 0.34.7 documentation](https://docs.rs/crate/sled/0.34.7)
