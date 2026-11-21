-- Add explicit archive metadata without changing existing task identity or workflow state.
ALTER TABLE tasks ADD COLUMN archived_at TEXT;
ALTER TABLE tasks ADD COLUMN archived_by TEXT;

CREATE INDEX idx_tasks_archive ON tasks(archived_at, task_id);

CREATE TRIGGER tasks_archive_metadata_insert
BEFORE INSERT ON tasks
WHEN (NEW.archived_at IS NULL) <> (NEW.archived_by IS NULL)
BEGIN
    SELECT RAISE(ABORT, 'archive metadata must be set together');
END;

CREATE TRIGGER tasks_archive_metadata_update
BEFORE UPDATE OF archived_at, archived_by ON tasks
WHEN (NEW.archived_at IS NULL) <> (NEW.archived_by IS NULL)
BEGIN
    SELECT RAISE(ABORT, 'archive metadata must be set together');
END;
