-- The clean-database snapshot already includes this schema; keep replay additive.
CREATE TABLE IF NOT EXISTS feedback_issues (
    id VARCHAR(36) PRIMARY KEY,
    owner_id VARCHAR(36) REFERENCES users(id) ON DELETE SET NULL,
    title VARCHAR(200) NOT NULL CHECK (length(btrim(title)) > 0),
    description TEXT NOT NULL CHECK (octet_length(description) BETWEEN 1 AND 20000),
    category VARCHAR(32) NOT NULL CHECK (category IN ('bug','model','access','suggestion','other')),
    model VARCHAR(200), request_id VARCHAR(128),
    status VARCHAR(16) NOT NULL CHECK (status IN ('open','resolved','closed')),
    version BIGINT NOT NULL CHECK (version > 0),
    activity_seq BIGINT NOT NULL,
    created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL,
    creation_key VARCHAR(36) NOT NULL, creation_hash VARCHAR(64) NOT NULL,
    CONSTRAINT uq_feedback_creation UNIQUE (owner_id,creation_key)
);
CREATE TABLE IF NOT EXISTS feedback_events (
    seq BIGSERIAL PRIMARY KEY,
    issue_id VARCHAR(36) NOT NULL REFERENCES feedback_issues(id) ON DELETE CASCADE,
    actor_id VARCHAR(36) REFERENCES users(id) ON DELETE SET NULL,
    actor_role VARCHAR(32) NOT NULL,
    kind VARCHAR(16) NOT NULL CHECK (kind IN ('created','reply','status')),
    content TEXT CHECK (octet_length(content) <= 20000),
    from_status VARCHAR(16), to_status VARCHAR(16), created_at BIGINT NOT NULL,
    client_request_id VARCHAR(36), payload_hash VARCHAR(64),
    CONSTRAINT uq_feedback_command UNIQUE (actor_id,client_request_id)
);
CREATE INDEX IF NOT EXISTS idx_feedback_owner_activity ON feedback_issues(owner_id,activity_seq);
CREATE INDEX IF NOT EXISTS idx_feedback_activity ON feedback_issues(activity_seq);
CREATE INDEX IF NOT EXISTS idx_feedback_events_issue ON feedback_events(issue_id,seq);
