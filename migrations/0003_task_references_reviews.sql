-- Generic task references, append-only reviews, and final commit marker.
ALTER TABLE tasks ADD COLUMN revision_count INTEGER NOT NULL DEFAULT 0 CHECK (revision_count >= 0);
ALTER TABLE tasks ADD COLUMN final_commit_sha TEXT CHECK (
    final_commit_sha IS NULL OR
    (length(final_commit_sha) IN (40, 64) AND final_commit_sha NOT GLOB '*[^0-9a-f]*')
);

CREATE TABLE task_external_references (
    reference_id TEXT PRIMARY KEY CHECK (length(trim(reference_id)) BETWEEN 1 AND 128),
    task_id INTEGER NOT NULL REFERENCES tasks(task_id) ON DELETE RESTRICT,
    namespace TEXT NOT NULL CHECK (length(trim(namespace)) BETWEEN 1 AND 128),
    value TEXT NOT NULL CHECK (length(trim(value)) BETWEEN 1 AND 512),
    relation TEXT NOT NULL CHECK (length(trim(relation)) BETWEEN 1 AND 128),
    scope TEXT NOT NULL CHECK (scope IN ('task', 'project')),
    created_at TEXT NOT NULL,
    removed_at TEXT,
    removed_by TEXT,
    CHECK ((removed_at IS NULL) = (removed_by IS NULL))
);
CREATE UNIQUE INDEX idx_external_ref_task_active
    ON task_external_references(task_id, namespace, value)
    WHERE scope = 'task' AND removed_at IS NULL;
CREATE UNIQUE INDEX idx_external_ref_project_active
    ON task_external_references(namespace, value)
    WHERE scope = 'project' AND removed_at IS NULL;
CREATE INDEX idx_external_ref_task_history
    ON task_external_references(task_id, created_at, reference_id);

CREATE TABLE task_reviews (
    review_record_id INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id INTEGER NOT NULL REFERENCES tasks(task_id) ON DELETE RESTRICT,
    review_id TEXT NOT NULL CHECK (length(trim(review_id)) BETWEEN 1 AND 128),
    version_no INTEGER NOT NULL CHECK (version_no > 0),
    reviewer TEXT NOT NULL CHECK (length(trim(reviewer)) BETWEEN 1 AND 256),
    outcome TEXT NOT NULL CHECK (outcome IN ('approved', 'changes_requested')),
    comments_json TEXT NOT NULL CHECK (json_valid(comments_json)),
    actor_id TEXT NOT NULL CHECK (length(trim(actor_id)) > 0),
    created_at TEXT NOT NULL,
    UNIQUE(task_id, review_id),
    FOREIGN KEY(task_id, version_no)
        REFERENCES task_versions(task_id, version_no) ON DELETE RESTRICT
);
CREATE INDEX idx_task_reviews_task_record
    ON task_reviews(task_id, review_record_id DESC);
CREATE TRIGGER task_reviews_no_update
BEFORE UPDATE ON task_reviews
BEGIN
    SELECT RAISE(ABORT, 'task reviews are append-only');
END;
CREATE TRIGGER task_reviews_no_delete
BEFORE DELETE ON task_reviews
BEGIN
    SELECT RAISE(ABORT, 'task reviews are append-only');
END;
