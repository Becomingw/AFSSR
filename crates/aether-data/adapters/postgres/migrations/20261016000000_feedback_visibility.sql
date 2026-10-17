-- Additive: existing issues and event contents remain private. No history is rewritten.
ALTER TABLE feedback_issues ADD COLUMN IF NOT EXISTS visibility VARCHAR(16) NOT NULL DEFAULT 'private'
    CHECK (visibility IN ('public', 'private'));
ALTER TABLE feedback_events ADD COLUMN IF NOT EXISTS visibility VARCHAR(16) NOT NULL DEFAULT 'private'
    CHECK (visibility IN ('public', 'private'));
CREATE INDEX IF NOT EXISTS idx_feedback_public_activity ON feedback_issues(visibility,activity_seq);
