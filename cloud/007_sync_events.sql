-- Task 19: immutable cloud change-event ledger for incremental sync.
-- PostgreSQL only. Apply after cloud/002_sync_devices.sql.
-- Authentication, event validation, transactional ingestion and pull APIs
-- are separate tasks. Do not expose direct client writes to this table.
CREATE TABLE IF NOT EXISTS sync_events (
    sequence_id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES sync_workspaces(id),
    origin_installation_id UUID NOT NULL REFERENCES sync_devices(installation_id),
    event_id UUID NOT NULL,
    entity_type TEXT NOT NULL CHECK (length(trim(entity_type)) > 0),
    entity_id TEXT NOT NULL CHECK (length(trim(entity_id)) > 0),
    operation TEXT NOT NULL CHECK (operation IN ('upsert', 'delete')),
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT sync_events_idempotency UNIQUE (workspace_id, event_id),
    CONSTRAINT sync_events_payload_object CHECK (jsonb_typeof(payload) = 'object')
);

-- Pull events for one workspace using a durable sequence cursor.
CREATE INDEX IF NOT EXISTS idx_sync_events_workspace_sequence
    ON sync_events(workspace_id, sequence_id);

-- Cross-workspace consistency for the event's origin installation.
-- Existing device registration is scoped to exactly one workspace.
CREATE UNIQUE INDEX IF NOT EXISTS idx_sync_devices_workspace_installation
    ON sync_devices(workspace_id, installation_id);

ALTER TABLE sync_events
    ADD CONSTRAINT sync_events_origin_workspace_fk
    FOREIGN KEY (workspace_id, origin_installation_id)
    REFERENCES sync_devices(workspace_id, installation_id);
