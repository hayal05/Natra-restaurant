-- Task 13: Neon PostgreSQL device-registration schema.
-- Apply after cloud/001_sync_workspaces.sql. Not a local SQLite migration.
-- Installation identity is independent of user login; credentials,
-- device authentication and sync quotas will be implemented separately.
CREATE TABLE IF NOT EXISTS sync_devices (
    installation_id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES sync_workspaces(id),
    registered_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deactivated_at TIMESTAMPTZ,
    CONSTRAINT sync_devices_deactivation_order
        CHECK (deactivated_at IS NULL OR deactivated_at >= registered_at)
);

CREATE INDEX IF NOT EXISTS idx_sync_devices_workspace
    ON sync_devices(workspace_id);
