# Task lock protocol research

**Date:** 2026-11-21  
**Status:** Candidate design; not approved  
**Related:** `openspec/changes/mvp-task-core/specs/task-locking/spec.md`, `openspec/changes/mvp-task-core/design.md`

## 1. Problem

Todoing's accepted lock requirement is stronger than a conventional database write lock:

- one owner holds a task for the full agent work session;
- the owner can continue to read and mutate the task;
- every other client is denied both reads and mutations;
- lock acquisition must not race with a read already in progress;
- process failure must not leave a task locked forever.

SQLite WAL explicitly permits readers and a writer to proceed concurrently. redb uses MVCC to permit concurrent readers and a writer. Neither database's normal transaction model is sufficient to enforce task-level read exclusion by itself.

## 2. Candidate protocols

### A. Persisted lease only

Store task ID, owner identity, lease expiry, and heartbeat in the transactional database. Every operation checks the lease before accessing task data.

**Advantages**
- One source of truth for task lock state.
- Lease renewal and crash recovery are naturally represented as data.
- No separate lock-file namespace.

**Problem**
A read can begin before a lock acquisition commits and continue against its existing database snapshot after the lock is committed. This is particularly relevant to MVCC/WAL storage. A lease check alone does not provide a drain barrier for in-flight operations unless every read also participates in a separate coordination protocol.

**Assessment:** Insufficient as the complete protocol for strict read exclusion.

### B. OS lock held for the full agent session

Hold an exclusive OS file lock for the duration of the task work session. Other clients acquire shared locks before accessing the task.

**Advantages**
- The OS releases the lock when the owning process terminates.
- Shared/exclusive locks can provide a barrier between reads and lock acquisition.

**Problems**
- A CLI process that exits cannot hold the lock for the full agent session.
- If a separate keeper process holds the exclusive lock, subsequent CLI processes belonging to the owner need an explicit authorization/bypass mechanism.
- A long-running helper/daemon introduces lifecycle and communication complexity.

**Assessment:** Useful synchronization primitive, but awkward as the only persistent owner protocol.

### C. Hybrid persisted lease plus short-lived OS access barrier

Use the database as the source of truth for the long-lived owner lease. Use a per-task OS lock file only as a short-lived barrier around each task operation and around lock-state transitions.

This separates two responsibilities:

- **Lease:** who owns the task during the agent work session, and when that ownership expires.
- **Access barrier:** ensure that lock acquisition, release/recovery, and ordinary task operations cannot race.

#### Ordinary task operation

1. Open the stable lock file for the task.
2. Acquire a shared OS lock.
3. Read the persisted task lease.
4. If another owner holds a valid lease, reject the operation before reading task data.
5. If no lease exists, or the supplied owner credential matches, perform the operation.
6. Keep the shared OS lock until the operation result has been materialized and emitted.
7. Release the OS lock.

Shared locks allow ordinary operations to coexist. The storage engine still governs transactional writes.

#### Acquire a task work-session lock

1. Acquire an exclusive OS lock for the task. This waits for all already-running shared-guarded operations to finish.
2. In a database transaction, inspect the existing lease.
3. Reject acquisition if another valid owner holds the task.
4. If the previous lease expired, record recovery and clear the stale lease.
5. Create the new owner lease and commit it.
6. Release the OS lock.

New operations can then acquire a shared OS lock, but will see the active lease and reject access unless they prove they are the owner.

#### Heartbeat, release, and recovery

- Heartbeat and release take the exclusive OS barrier and update the lease in a database transaction.
- An expired lease is recovered only while holding the exclusive barrier, after in-flight operations have drained.
- If the owner process crashes, the OS releases its short-lived operation lock automatically. The persisted lease remains until expiry, then can be recovered by a later operation.
- Lock files are stable coordination objects. They must not be deleted, renamed, or replaced while clients may be using them.

**Advantages**
- The database retains authoritative owner and expiry state.
- The OS barrier handles in-flight operation exclusion.
- No process must remain alive for the whole agent session.
- The protocol can work with either SQLite or redb, subject to OS-lock verification.

**Costs and risks**
- Every task operation must use the same guard protocol; bypasses break the guarantee.
- A new cross-platform file-lock dependency or a sufficiently recent Rust standard library may be required.
- Owner credentials must be passed securely to later CLI invocations.
- File locks are advisory on common platforms; all Todoing clients must cooperate and use the same stable lock-file path.
- Remote filesystem semantics must not be assumed.

**Assessment:** Best candidate for a prototype. It directly addresses the in-flight read race while keeping the long-lived lease in transactional storage.

## 3. Platform evidence

Rust's standard `std::fs::File` API documents shared and exclusive file locks. The current documentation maps these to `flock` on Unix and `LockFileEx` on Windows. The API is stable from Rust 1.89.0. The documentation also warns that lock interactions and filesystem behavior are platform-specific.

The `filelock` crate documents shared and exclusive advisory locks, using `flock` on Unix and `LockFileEx` on Windows. Its documentation requires all participants to use the same stable lock-file path and warns against deleting or replacing lock files while participants may be running. It also advises validating remote-filesystem semantics.

The repository currently uses Rust edition 2024 but does not declare an explicit MSRV. The choice between the standard-library API and a crate must account for the eventual MSRV policy.

## 4. Unresolved protocol details

1. Owner identity versus owner credential: what is displayed/audited and what authorizes access.
2. How the owner credential is supplied to each CLI call without leaking into shell history or process listings.
3. Lease duration, heartbeat interval, and expiry policy.
4. Whether lock acquisition waits, times out, or returns a conflict when another operation is active.
5. How list/filter operations handle tasks locked by another owner: omit them or fail the entire query.
6. Whether task history/version reads are also denied to non-owners; the accepted wording says task operations, including reads, are blocked.
7. How clock changes and system suspend/resume affect lease expiry.
8. How the protocol behaves when the project directory is copied, moved, or accessed through a remote filesystem.
9. How a crashed owner is represented in the immutable operation history.

## 5. Required tests

- Two independent processes: shared reads coexist on an unlocked task.
- Lock acquisition waits for an already-running read to finish.
- Once acquisition returns, another process cannot read task body, metadata, history, or versions.
- The owner can read and mutate using its credential.
- A non-owner mutation is rejected while the lease is valid.
- Concurrent acquisition attempts yield exactly one owner.
- Heartbeat prevents valid lease expiry.
- Explicit release makes the task available.
- Owner process termination releases the OS barrier; the persisted lease remains until expiry.
- Expired lease recovery is serialized against reads and writes.
- A lock file is never unlinked or replaced during active use.
- Repeat the tests on Windows and Linux with separate processes.

No prototype or platform test has been executed. The hybrid protocol is a candidate for implementation testing, not an accepted architecture decision.

## 6. Sources

- [Rust std::fs::File locking](https://doc.rust-lang.org/std/fs/struct.File.html)
- [filelock crate](https://docs.rs/filelock/latest/filelock/)
- [fslock crate](https://docs.rs/fslock/latest/fslock/)
- [SQLite WAL concurrency](https://www.sqlite.org/wal.html)
- [redb multi-process concurrency design](https://github.com/cberner/redb/blob/master/docs/design.md)
