-- Additive. Human-friendly issue numbers (#N) for cross references and administrator pinning.
ALTER TABLE feedback_issues ADD COLUMN IF NOT EXISTS number BIGINT;
ALTER TABLE feedback_issues ADD COLUMN IF NOT EXISTS pinned_at BIGINT;
-- Number existing issues once, in creation order; later issues are numbered by the application.
UPDATE feedback_issues f
SET number = r.n
FROM (
    SELECT id, ROW_NUMBER() OVER (ORDER BY created_at, id) AS n
    FROM feedback_issues
) r
WHERE f.id = r.id AND f.number IS NULL;
ALTER TABLE feedback_issues ALTER COLUMN number SET NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS uq_feedback_number ON feedback_issues(number);
CREATE INDEX IF NOT EXISTS idx_feedback_pinned ON feedback_issues(pinned_at);
-- Pin changes are recorded as events, so the event kind check must allow them.
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
    CHECK (kind IN ('created','reply','status','pin'));
