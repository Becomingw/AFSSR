-- Additive. Per-user opt-out from showing the username on the public leaderboard.
-- Defaults to FALSE: usernames are shown unless the user opts into anonymity (#2).
ALTER TABLE user_preferences
    ADD COLUMN IF NOT EXISTS leaderboard_anonymous BOOLEAN NOT NULL DEFAULT FALSE;
