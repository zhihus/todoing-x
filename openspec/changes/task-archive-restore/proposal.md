# Proposal: Task archive and restore

## Current behavior

The SQLite task store supports stable task identity, completion/cancellation outcomes, history, versions, dependencies, and locks. It does not have an archive state or archive/restore operations. Active task lists therefore have no separate archive view.

## Intended change

Add explicit task archiving and restoration as lifecycle operations independent of workflow status and terminal outcome.

- Archiving removes a task from the default active list without deleting its record.
- Archived tasks remain available through an archive view and direct task inspection when not locked.
- Task history, versions, dependencies, Git references, and identity remain intact.
- Restoring returns a task to the active list without changing its workflow status or terminal outcome.
- Archiving and restoration are explicit user/agent actions; neither completion nor cancellation triggers them automatically.

## Out of scope

- Automatic archival policies based on age, task count, or status.
- Archive retention, purge, or physical deletion.
- Bulk archive/restore.
- A graphical archive view.
- Import/export of archived records beyond the existing database snapshot behavior.

## Product basis

PRODUCT.md §2 and §5 define archive and restore as Todoing responsibilities, separate from status and completion/cancellation. Archived tasks are removed from the active list, retain history and relationships, can be restored, and remain readable by agents when not locked. Automatic archival policies are explicitly outside the MVP.
