# Storage engine technical evaluation

**Date:** 2026-11-21  
**Status:** Research findings; no storage engine selected  
**Scope:** Embedded, project-local storage for the Todoing MVP

## 1. Decision context

The accepted product requirements need more than atomic replacement of individual files. A single successful task mutation must atomically update the task, version, append-only history, and associated indexes/state. The store must also support concurrent CLI processes, stable ID allocation, dependencies, and lock metadata.

The workload is local-first and project-scoped. Manifest currently runs agents sequentially, but Todoing must remain correct when multiple independent clients access the same project store. A central database server is outside the MVP.

The current Markdown/YAML files and JSON index are migration inputs. Their current format does not require the future primary store to remain file-per-task.

## 2. Candidate comparison

| Candidate | Transaction and concurrency model | Data model | Portability and operational considerations | Initial assessment |
|---|---|---|---|---|
| SQLite via rusqlite | ACID transactions; multiple readers, one writer at a time. WAL allows readers and a writer to proceed concurrently, but still permits only one writer. | Relational SQL; natural fit for tasks, dependencies, versions, history, status definitions, and indexes. | Mature cross-platform engine. rusqlite's bundled feature avoids depending on a system-installed SQLite library; optional backup feature exposes SQLite's online backup API. WAL requires all processes to be on the same host and is unsuitable for network filesystems. | Strongest initial fit; retain as leading candidate pending prototype and failure testing. |
| redb | ACID transactions, MVCC readers, crash safety. Supports concurrent readers and a writer. Current documentation describes exclusive-writer, single-writer, and multi-writer process modes. | Embedded key-value store using copy-on-write B+trees; application must define record and secondary-index layout. | Pure Rust. Multi-process single/multi-writer modes use byte-range file locks and are documented for Linux, Apple platforms, and Windows; other platforms fall back to exclusive-writer behavior. Processes sharing a database must use compatible locking protocol generations. | Serious alternative, especially if pure Rust and MVCC are priorities; requires more application-level data/index design. |
| LMDB via heed | ACID transactions; multiple readers and one writer. | Memory-mapped key-value database; application defines key layout and indexes. | Mature low-level engine, but mmap and file-locking constraints matter. heed documents that long-lived read transactions prevent reuse of freed pages and can cause database growth; remote filesystems are unsupported. | Technically viable, but adds operational constraints without a clear advantage over SQLite for this product. |
| sled | Documentation describes atomic operations and ACID transactions across multiple items/trees. | Embedded key-value store; application-managed schema and indexes. | The reviewed documentation did not establish a sufficiently clear cross-process locking contract for Todoing's multi-process requirements. | Do not shortlist for selection until cross-process guarantees and maintenance/release posture are verified. |
| Markdown/YAML + JSON files | Atomic replacement can protect individual files, but the current implementation does not provide one transaction across task, index, history, and version records. | Human-readable files and a rebuildable index. | Portable and easy to inspect, but requires custom transaction journals, locking, recovery, and index coordination to meet the target guarantees. | Retain as migration/interchange format, not the target transactional primary store. |

## 3. Findings

### 3.1 SQLite and rusqlite

SQLite transactions provide atomic commit/rollback. SQLite serializes writes: only one write transaction can be active at a time, while multiple read transactions may coexist. WAL mode improves reader/writer concurrency, but does not create parallel writers.

For Todoing, a single-writer model is likely sufficient for the expected local project workload. It simplifies ID allocation and multi-record mutation: task state, version, history, dependencies, and indexes can be committed within one database transaction. This remains a hypothesis until measured under concurrent CLI workloads.

rusqlite is the established Rust binding. Its current documentation identifies version 0.43.0 and supports a bundled SQLite build, avoiding reliance on the user's system SQLite installation. The crate also exposes an optional feature for SQLite's online backup API.

WAL has a material deployment constraint: all processes using the database must be on the same host. It must not be treated as a safe multi-host/network-filesystem mode. The project store should therefore document local-filesystem expectations if WAL is selected.

### 3.2 redb

redb is a pure-Rust embedded ACID key-value database. Its documentation describes MVCC concurrent readers, crash-safe transactions, and multi-process modes. The current 5.0 documentation states that byte-range-lock-based single-writer and multi-writer modes are supported on Linux, Apple platforms, and Windows; the exclusive-writer mode uses a whole-file lock and is available more broadly.

This is a meaningful alternative, not merely a lightweight cache. Its main trade-off for Todoing is the absence of a relational query model: dependency traversal, filtering, secondary indexes, status configuration, and history queries become application-managed structures. That is feasible, but it increases the amount of correctness-critical storage code Todoing must own.

The multi-process locking protocol is version-sensitive. Every process opening the same database concurrently must use compatible protocol generations. The 4.5 release introduced the newer process concurrency modes, and the 5.0 release is dated 2026-11-18. This recent evolution warrants explicit compatibility and upgrade tests before adoption.

### 3.3 LMDB through heed

heed provides a Rust wrapper around LMDB. LMDB offers transactional key-value storage and concurrent read transactions with a single writer. The operational constraint most relevant to Todoing is reader lifetime: long-lived read transactions prevent reuse of pages freed by newer writes and can cause the database to grow. The heed documentation also warns against remote filesystems and explains mmap-related safety constraints.

Todoing's CLI operations should be short-lived, which reduces this risk, but the full-session task lock must not be implemented as a long-lived database read transaction. LMDB's model would therefore not remove the need for a separate lease/lock protocol.

### 3.4 Why the current file store is insufficient

The current implementation atomically replaces each task or index file individually. A task write followed by an index write is not one atomic logical operation. Adding separate history and version files would multiply partial-failure states and require a custom journal/recovery protocol.

A file-based design could be made transactional, but it would require substantially more bespoke infrastructure than using an embedded transactional engine. It should not be retained as the primary store merely for format continuity.

## 4. Important distinction: database concurrency versus task locks

A storage engine's transaction locks do not implement Todoing's task work-session lock.

The product requirement is stronger than ordinary database isolation: while one owner holds a task lock, other clients must not perform task operations on that task, including reads. SQLite WAL and redb MVCC intentionally allow readers to coexist with writers. Their concurrency models therefore do not, by themselves, satisfy task-level read exclusion.

The architecture must define an application-level lock protocol that every task read and mutation uses. It must also define what happens to reads already in progress when a lock is acquired, how the owner proves its identity on subsequent CLI calls, how lease renewal works, and how expired locks are recovered. These points remain open and must be tested across separate OS processes, not only threads.

## 5. Preliminary recommendation

Keep SQLite via rusqlite as the leading candidate for the first architecture prototype.

Reasons:

1. The task model is relational: tasks, dependencies, versions, history, statuses, and lock records have explicit relationships.
2. SQLite provides the required multi-record transaction boundary without a custom transaction journal.
3. Its single-writer model is compatible with the expected local-first, sequential-agent workload, subject to contention tests.
4. SQL constraints and indexes can reduce the amount of custom consistency code.
5. rusqlite provides a bundled build option and an online backup API integration.

Keep redb as the comparison candidate. Its pure-Rust implementation, ACID transactions, MVCC, and multi-process support are strong advantages. A short prototype should compare schema/index complexity, cross-process behavior, recovery, and Windows/Linux builds before final selection.

**This is not an architecture decision.** SQLite remains preliminary until the prototype and failure tests below are completed and the decision register is explicitly updated.

## 6. Required prototype and verification work

1. Build minimal SQLite/rusqlite and redb prototypes on Windows and Linux.
2. Run concurrent task creation from multiple processes; verify unique IDs and no lost writes.
3. Run conflicting task mutations from multiple processes; verify deterministic serialization or conflict errors.
4. Inject process termination around task/version/history commits; verify all-or-nothing recovery.
5. Test dependency and history updates in the same logical transaction as task mutation.
6. Test lock acquisition against active reads, competing reads, writes, heartbeat renewal, release, process crash, and lease expiry.
7. Verify that all CLI operations enforce task locks consistently, including list/filter behavior where locked task data could otherwise be exposed.
8. Test backup and restore while the database is active, including integrity validation after restore.
9. Measure realistic task/history sizes and concurrent access patterns; do not optimize from synthetic microbenchmarks alone.
10. Verify database upgrade/migration behavior and the preservation of legacy IDs, Git references, task content, and metadata.

No prototype, benchmark, cross-platform build, or crash-injection test has been executed as part of this research note.

## 7. Sources

- [SQLite transactions](https://www.sqlite.org/lang_transaction.html)
- [SQLite isolation and write serialization](https://www.sqlite.org/isolation.html)
- [SQLite Write-Ahead Logging](https://www.sqlite.org/wal.html)
- [SQLite atomic commit](https://www.sqlite.org/atomiccommit.html)
- [SQLite online backup API](https://www.sqlite.org/backup.html)
- [rusqlite 0.43.0 documentation](https://docs.rs/crate/rusqlite/latest)
- [redb 5.0.0 documentation](https://docs.rs/redb/latest/redb/)
- [redb multi-process concurrency design](https://github.com/cberner/redb/blob/master/docs/design.md)
- [heed documentation](https://docs.rs/heed/latest/heed/)
- [heed read transaction limitations](https://docs.rs/heed/latest/heed/struct.RoTxn.html)
- [sled documentation](https://docs.rs/sled/latest/sled/)
