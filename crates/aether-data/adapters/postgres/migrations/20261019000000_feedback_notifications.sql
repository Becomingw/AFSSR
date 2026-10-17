-- Additive. In-app notifications for @mentions in feedback topics and replies.
CREATE TABLE IF NOT EXISTS feedback_notifications (
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
CREATE INDEX IF NOT EXISTS idx_feedback_notifications_user ON feedback_notifications(user_id, id);
