-- Task 14: record sync attempts and completed sessions for each installation.
-- Apply after cloud/002_sync_devices.sql; PostgreSQL only (not SQLite).
-- Quota enforcement, authentication and session lifecycle come in later tasks.
CREATE TABLE IF NOT EXISTS sync_sessions (
    id UUID PRIMARY KEY,
    installation_id UUID NOT NULL REFERENCES sync_devices(installation_id),
    started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ,
    status TEXT NOT NULL DEFAULT 'running'
        CHECK (status IN ('running', 'completed', 'failed')),
    CONSTRAINT sync_sessions_completion_consistency CHECK (
        (status = 'running' AND completed_at IS NULL)
        OR (status = 'completed' AND completed_at IS NOT NULL AND completed_at >= started_at)
        OR (status = 'failed')
    )
);

-- Efficiently count completed sessions on an EAT calendar day.
CREATE INDEX IF NOT EXISTS idx_sync_sessions_completed_by_device
    ON sync_sessions(installation_id, completed_at)
    WHERE status = 'completed';
