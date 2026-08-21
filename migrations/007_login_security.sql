-- 007_login_security.sql
-- Failed-login tracking for account lockout (see auth_service::login).
--
-- A local desktop app has no network-level rate limiting in front of
-- it, so without this, anyone with a few minutes alone at an unlocked
-- machine (or a script driving the UI) can brute-force a weak
-- password with no friction at all. This adds per-account lockout
-- after repeated failures — not a full IP/network throttle (there's
-- no network layer to throttle), but it makes automated guessing
-- against one account slow enough to be impractical.
--
-- Depends on: 001_initial.sql (users table)

PRAGMA foreign_keys = ON;

ALTER TABLE users ADD COLUMN failed_login_attempts INTEGER NOT NULL DEFAULT 0;
ALTER TABLE users ADD COLUMN locked_until TEXT;
