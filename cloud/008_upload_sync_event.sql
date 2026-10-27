-- Task 20: workspace-scoped, idempotent upload of one change event.
-- PostgreSQL only; apply after cloud/007_sync_events.sql.
-- Call exclusively through a trusted API that has authenticated the
-- installation and authorized the workspace. Never expose this as public SQL.
CREATE OR REPLACE FUNCTION upload_sync_event(
    p_workspace_id uuid,
    p_installation_id uuid,
    p_event_id uuid,
    p_entity_type text,
    p_entity_id text,
    p_operation text,
    p_payload jsonb
) RETURNS bigint
LANGUAGE plpgsql
AS $$
DECLARE
    v_sequence bigint;
    v_existing sync_events%ROWTYPE;
BEGIN
    IF p_workspace_id IS NULL OR p_installation_id IS NULL OR p_event_id IS NULL THEN
        RAISE EXCEPTION 'workspace, installation and event IDs are required';
    END IF;

    -- Serialize event commits per workspace: sequence-based pulls must not
    -- skip a lower sequence that commits after a higher one.
    PERFORM 1 FROM sync_workspaces WHERE id = p_workspace_id FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'workspace not found';
    END IF;

    PERFORM 1 FROM sync_devices
     WHERE installation_id = p_installation_id
       AND workspace_id = p_workspace_id
       AND deactivated_at IS NULL;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'installation is not active in this workspace';
    END IF;

    -- Same event ID with changed content is an error, not a silent overwrite.
    SELECT * INTO v_existing FROM sync_events
     WHERE workspace_id = p_workspace_id AND event_id = p_event_id;
    IF FOUND THEN
        IF v_existing.origin_installation_id IS DISTINCT FROM p_installation_id
           OR v_existing.entity_type IS DISTINCT FROM p_entity_type
           OR v_existing.entity_id IS DISTINCT FROM p_entity_id
           OR v_existing.operation IS DISTINCT FROM p_operation
           OR v_existing.payload IS DISTINCT FROM p_payload THEN
            RAISE EXCEPTION 'event ID already exists with different content';
        END IF;
        RETURN v_existing.sequence_id;
    END IF;

    INSERT INTO sync_events (
        workspace_id, origin_installation_id, event_id,
        entity_type, entity_id, operation, payload
    ) VALUES (
        p_workspace_id, p_installation_id, p_event_id,
        p_entity_type, p_entity_id, p_operation, p_payload
    ) RETURNING sequence_id INTO v_sequence;

    RETURN v_sequence;
END;
$$;
