-- Neon PostgreSQL schema: Task 7 (workspace foundation only).
-- Run in Neon manually after reviewing; this file is NOT a SQLite migration.
-- Device registration, authentication, events, and quota tables come later.
CREATE TABLE IF NOT EXISTS sync_workspaces (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
