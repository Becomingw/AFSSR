-- Additive. Editing and deleting feedback: issues and replies keep their rows (so numbers are never
-- reused and idempotency keys stay valid); changes are recorded as events for incremental readers.
ALTER TABLE feedback_issues ADD COLUMN IF NOT EXISTS edited_at BIGINT;
ALTER TABLE feedback_issues ADD COLUMN IF NOT EXISTS deleted_at BIGINT;
ALTER TABLE feedback_events ADD COLUMN IF NOT EXISTS target_seq BIGINT;
ALTER TABLE feedback_events ADD COLUMN IF NOT EXISTS edited_at BIGINT;
ALTER TABLE feedback_events ADD COLUMN IF NOT EXISTS deleted_at BIGINT;
DO $$
DECLARE c RECORD;
BEGIN
    FOR c IN
        SELECT conname FROM pg_constraint
        WHERE conrelid = 'feedback_events'::regclass AND contype = 'c'
          AND pg_get_constraintdef(oid) LIKE '%kind%'
    LOOP
        EXECUTE format('ALTER TABLE feedback_events DROP CONSTRAINT %I', c.conname);
    END LOOP;
END $$;
ALTER TABLE feedback_events ADD CONSTRAINT feedback_events_kind_check
    CHECK (kind IN ('created','reply','status','pin','issue_edit','issue_delete','reply_edit','reply_delete'));
