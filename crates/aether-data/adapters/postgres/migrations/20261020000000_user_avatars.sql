-- Additive. One profile picture per user, kept in the database (small, already re-encoded by the
-- console) so nobody's browser has to load pictures from third-party hosts.
CREATE TABLE IF NOT EXISTS user_avatars (
    user_id VARCHAR(36) PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    content_type VARCHAR(32) NOT NULL CHECK (content_type IN ('image/png','image/jpeg','image/webp')),
    data BYTEA NOT NULL CHECK (octet_length(data) BETWEEN 1 AND 262144),
    updated_at BIGINT NOT NULL
);
