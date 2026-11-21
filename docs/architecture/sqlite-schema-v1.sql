-- Todoing SQLite schema v1 (design baseline; applied through numbered migrations).
-- Every connection MUST execute PRAGMA foreign_keys=ON and verify it is enabled.
-- Selected database mode: journal_mode=DELETE, synchronous=FULL.

CREATE TABLE schema_migrations (
    version       INTEGER PRIMARY KEY,
    checksum      TEXT NOT NULL,
    applied_at    TEXT NOT NULL
);

CREATE TABLE store_metadata (
    key           TEXT PRIMARY KEY,
    value         TEXT NOT NULL
);

CREATE TABLE tasks (
    task_id       INTEGER PRIMARY KEY AUTOINCREMENT,
    title         TEXT NOT NULL CHECK (length(trim(title)) > 0),
    status_id     TEXT NOT NULL,
    outcome       TEXT CHECK (outcome IS NULL OR outcome IN ('completed', 'cancelled')),
    body          TEXT NOT NULL DEFAULT '',
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL
);

CREATE TABLE task_labels (
    task_id       INTEGER NOT NULL REFERENCES tasks(task_id) ON DELETE RESTRICT,
    position      INTEGER NOT NULL CHECK (position >= 0),
    label         TEXT NOT NULL CHECK (length(trim(label)) > 0),
    label_key     TEXT NOT NULL,
    PRIMARY KEY (task_id, position),
    UNIQUE (task_id, label_key)
);

CREATE TABLE task_commit_refs (
    task_id       INTEGER NOT NULL REFERENCES tasks(task_id) ON DELETE RESTRICT,
    position      INTEGER NOT NULL CHECK (position >= 0),
    sha           TEXT NOT NULL CHECK (
        length(sha) IN (40, 64) AND sha NOT GLOB '*[^0-9a-f]*'
    ),
    PRIMARY KEY (task_id, position),
    UNIQUE (task_id, sha)
);

CREATE TABLE task_dependencies (
    task_id             INTEGER NOT NULL REFERENCES tasks(task_id) ON DELETE RESTRICT,
    depends_on_task_id  INTEGER NOT NULL REFERENCES tasks(task_id) ON DELETE RESTRICT,
    created_at          TEXT NOT NULL,
    PRIMARY KEY (task_id, depends_on_task_id),
    CHECK (task_id <> depends_on_task_id)
);

CREATE TABLE task_versions (
    version_id     INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id        INTEGER NOT NULL REFERENCES tasks(task_id) ON DELETE RESTRICT,
    version_no     INTEGER NOT NULL CHECK (version_no > 0),
    snapshot_json  TEXT NOT NULL,
    created_at     TEXT NOT NULL,
    actor_id       TEXT NOT NULL CHECK (length(trim(actor_id)) > 0),
    UNIQUE (task_id, version_no),
    UNIQUE (task_id, version_id)
);

CREATE TABLE task_baselines (
    task_id       INTEGER PRIMARY KEY REFERENCES tasks(task_id) ON DELETE RESTRICT,
    version_id    INTEGER NOT NULL,
    approved_at   TEXT NOT NULL,
    approved_by   TEXT NOT NULL CHECK (length(trim(approved_by)) > 0),
    FOREIGN KEY (task_id, version_id)
        REFERENCES task_versions(task_id, version_id) ON DELETE RESTRICT
);

CREATE TABLE operation_history (
    event_id             INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id              INTEGER REFERENCES tasks(task_id) ON DELETE RESTRICT,
    actor_id             TEXT NOT NULL CHECK (length(trim(actor_id)) > 0),
    occurred_at          TEXT NOT NULL,
    operation            TEXT NOT NULL,
    previous_json        TEXT,
    requested_json       TEXT,
    outcome              TEXT NOT NULL CHECK (outcome IN ('succeeded', 'rejected')),
    error_code           TEXT,
    details_json         TEXT,
    config_revision      TEXT,
    fencing_generation   INTEGER,
    CHECK (
        (outcome = 'succeeded' AND error_code IS NULL)
        OR
        (outcome = 'rejected' AND error_code IS NOT NULL)
    )
);

-- A released row is retained so fencing_generation never resets for a task.
-- A missing row means that the task has never been locked.
CREATE TABLE task_locks (
    task_id             INTEGER PRIMARY KEY REFERENCES tasks(task_id) ON DELETE RESTRICT,
    state               TEXT NOT NULL CHECK (state IN ('active', 'released')),
    owner_id            TEXT,
    token_hash          BLOB,
    acquired_at         INTEGER NOT NULL,
    heartbeat_at        INTEGER NOT NULL,
    expires_at          INTEGER,
    released_at         INTEGER,
    fencing_generation  INTEGER NOT NULL CHECK (fencing_generation > 0),
    CHECK (
        (state = 'active'
            AND owner_id IS NOT NULL
            AND token_hash IS NOT NULL
            AND length(token_hash) = 32
            AND expires_at IS NOT NULL
            AND released_at IS NULL)
        OR
        (state = 'released'
            AND owner_id IS NULL
            AND token_hash IS NULL
            AND expires_at IS NULL
            AND released_at IS NOT NULL)
    )
);

CREATE INDEX idx_tasks_status ON tasks(status_id, task_id);
CREATE INDEX idx_tasks_outcome ON tasks(outcome, task_id);
CREATE INDEX idx_tasks_updated ON tasks(updated_at, task_id);
CREATE INDEX idx_versions_task_no ON task_versions(task_id, version_no DESC);
CREATE INDEX idx_history_task_event ON operation_history(task_id, event_id DESC);
CREATE INDEX idx_dependencies_reverse ON task_dependencies(depends_on_task_id, task_id);
CREATE INDEX idx_locks_state_expiry ON task_locks(state, expires_at);

-- Task records are never physically deleted.
CREATE TRIGGER tasks_no_delete
BEFORE DELETE ON tasks
BEGIN
    SELECT RAISE(ABORT, 'physical task deletion is prohibited');
END;

-- Versions and operation history are immutable.
CREATE TRIGGER task_versions_no_update
BEFORE UPDATE ON task_versions
BEGIN
    SELECT RAISE(ABORT, 'task versions are immutable');
END;

CREATE TRIGGER task_versions_no_delete
BEFORE DELETE ON task_versions
BEGIN
    SELECT RAISE(ABORT, 'task versions are immutable');
END;

CREATE TRIGGER operation_history_no_update
BEFORE UPDATE ON operation_history
BEGIN
    SELECT RAISE(ABORT, 'operation history is append-only');
END;

CREATE TRIGGER operation_history_no_delete
BEFORE DELETE ON operation_history
BEGIN
    SELECT RAISE(ABORT, 'operation history is append-only');
END;

-- Initial metadata values are inserted by the store initializer:
-- schema_version = "1"
-- project_id = stable project identifier
