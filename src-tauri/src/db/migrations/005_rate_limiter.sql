-- Migration 005: Rate limiter persistence
--
-- Persists login attempt tracking across application restarts
-- to prevent brute-force attacks from being reset by server restart.

CREATE TABLE IF NOT EXISTS rate_limiter_attempts (
    key TEXT PRIMARY KEY,
    count INTEGER NOT NULL DEFAULT 0,
    successful_count INTEGER NOT NULL DEFAULT 0,
    total_failed_count INTEGER NOT NULL DEFAULT 0,
    first_attempt_at INTEGER NOT NULL,
    last_attempt_at INTEGER NOT NULL
);
