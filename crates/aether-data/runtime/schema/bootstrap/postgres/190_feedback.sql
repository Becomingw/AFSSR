CREATE TABLE feedback_issues (
    id VARCHAR(36) PRIMARY KEY,
    owner_id VARCHAR(36) REFERENCES users(id) ON DELETE SET NULL,
    title VARCHAR(200) NOT NULL CHECK (length(btrim(title)) > 0),
    description TEXT NOT NULL CHECK (octet_length(description) BETWEEN 1 AND 20000),
    category VARCHAR(32) NOT NULL CHECK (category IN ('bug','model','access','suggestion','other')),
    model VARCHAR(200), request_id VARCHAR(128),
    status VARCHAR(16) NOT NULL CHECK (status IN ('open','resolved','closed')),
    visibility VARCHAR(16) NOT NULL DEFAULT 'private' CHECK (visibility IN ('public','private')),
    number BIGINT NOT NULL,
    pinned_at BIGINT,
    edited_at BIGINT,
    deleted_at BIGINT,
    version BIGINT NOT NULL CHECK (version > 0),
    activity_seq BIGINT NOT NULL,
    created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL,
    creation_key VARCHAR(36) NOT NULL, creation_hash VARCHAR(64) NOT NULL,
    CONSTRAINT uq_feedback_creation UNIQUE (owner_id,creation_key),
    CONSTRAINT uq_feedback_number UNIQUE (number)
);
CREATE TABLE feedback_events (
    seq BIGSERIAL PRIMARY KEY,
    issue_id VARCHAR(36) NOT NULL REFERENCES feedback_issues(id) ON DELETE CASCADE,
    actor_id VARCHAR(36) REFERENCES users(id) ON DELETE SET NULL,
    actor_role VARCHAR(32) NOT NULL,
    kind VARCHAR(16) NOT NULL CHECK (kind IN ('created','reply','status','pin','issue_edit','issue_delete','reply_edit','reply_delete')),
    visibility VARCHAR(16) NOT NULL DEFAULT 'private' CHECK (visibility IN ('public','private')),
    content TEXT CHECK (octet_length(content) <= 20000),
    from_status VARCHAR(16), to_status VARCHAR(16), created_at BIGINT NOT NULL,
    client_request_id VARCHAR(36), payload_hash VARCHAR(64),
    target_seq BIGINT, edited_at BIGINT, deleted_at BIGINT,
    CONSTRAINT uq_feedback_command UNIQUE (actor_id,client_request_id)
);
CREATE INDEX idx_feedback_owner_activity ON feedback_issues(owner_id,activity_seq);
CREATE INDEX idx_feedback_activity ON feedback_issues(activity_seq);
CREATE INDEX idx_feedback_public_activity ON feedback_issues(visibility,activity_seq);
CREATE INDEX idx_feedback_pinned ON feedback_issues(pinned_at);
CREATE INDEX idx_feedback_events_issue ON feedback_events(issue_id,seq);
CREATE TABLE feedback_notifications (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(36) NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind VARCHAR(16) NOT NULL CHECK (kind IN ('mention')),
    issue_id VARCHAR(36) NOT NULL REFERENCES feedback_issues(id) ON DELETE CASCADE,
    event_seq BIGINT NOT NULL,
    actor_id VARCHAR(36) REFERENCES users(id) ON DELETE SET NULL,
    snippet TEXT NOT NULL CHECK (octet_length(snippet) <= 2000),
    created_at BIGINT NOT NULL,
    read_at BIGINT,
    CONSTRAINT uq_feedback_notification UNIQUE (user_id, issue_id, event_seq, kind)
);
CREATE INDEX idx_feedback_notifications_user ON feedback_notifications(user_id, id);
CREATE TABLE user_avatars (
    user_id VARCHAR(36) PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    content_type VARCHAR(32) NOT NULL CHECK (content_type IN ('image/png','image/jpeg','image/webp')),
    data BYTEA NOT NULL CHECK (octet_length(data) BETWEEN 1 AND 262144),
    updated_at BIGINT NOT NULL
);
