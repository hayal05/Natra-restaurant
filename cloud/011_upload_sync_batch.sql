-- Task 36: bounded, atomic upload under the authenticated session scope.
-- PostgreSQL only; apply after 007/008. Never invoked by a migration locally.
CREATE OR REPLACE FUNCTION public.upload_sync_batch(
    p_workspace_id uuid, p_installation_id uuid, p_session_id uuid, p_events jsonb
) RETURNS TABLE(event_id uuid, sequence_id bigint)
LANGUAGE plpgsql
SECURITY INVOKER
SET search_path = pg_catalog, public
AS $$
DECLARE
    v_session public.sync_sessions%ROWTYPE;
    v_event jsonb;
    v_event_id uuid;
    v_sequence bigint;
BEGIN
    IF p_workspace_id IS NULL OR p_installation_id IS NULL OR p_session_id IS NULL THEN
        RAISE EXCEPTION 'authenticated scope is required' USING ERRCODE='42501';
    END IF;
    IF p_events IS NULL OR jsonb_typeof(p_events) <> 'array' THEN
        RAISE EXCEPTION 'event array is required' USING ERRCODE='22023';
    END IF;
    IF jsonb_array_length(p_events) NOT BETWEEN 1 AND 500 OR
       octet_length(p_events::text) > 1048576 THEN
        RAISE EXCEPTION 'upload batch limit exceeded' USING ERRCODE='22023';
    END IF;
    -- Parse UUIDs before writing; case variants cannot bypass duplicate checks.
    IF (SELECT count(DISTINCT (e->>'event_id')::uuid) FROM jsonb_array_elements(p_events) AS e)
        <> jsonb_array_length(p_events) THEN
        RAISE EXCEPTION 'duplicate or missing event ID' USING ERRCODE='22023';
    END IF;

    -- Match upload_sync_event's workspace-first ordering. Device/session locks
    -- then prevent completion/deactivation while the authorized batch commits.
    PERFORM 1 FROM public.sync_workspaces WHERE id=p_workspace_id FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'workspace unavailable' USING ERRCODE='42501';
    END IF;
    PERFORM 1 FROM public.sync_devices
     WHERE workspace_id=p_workspace_id AND installation_id=p_installation_id
       AND deactivated_at IS NULL FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'device unavailable' USING ERRCODE='42501';
    END IF;
    SELECT * INTO v_session FROM public.sync_sessions
     WHERE id=p_session_id AND installation_id=p_installation_id FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'session unavailable' USING ERRCODE='42501';
    END IF;
    IF v_session.status <> 'running' OR v_session.completed_at IS NOT NULL OR
       v_session.started_at > clock_timestamp() OR
       v_session.started_at <= clock_timestamp()-interval '15 minutes' THEN
        RAISE EXCEPTION 'session unavailable' USING ERRCODE='42501';
    END IF;

    FOR v_event IN SELECT value FROM jsonb_array_elements(p_events) LOOP
        IF jsonb_typeof(v_event) <> 'object' OR
           NOT (v_event ?& ARRAY['event_id','entity_type','entity_id','operation','payload']) OR
           (v_event - ARRAY['event_id','entity_type','entity_id','operation','payload']) <> '{}'::jsonb OR
           jsonb_typeof(v_event->'event_id') <> 'string' OR
           (v_event->>'event_id') !~* '^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$' OR
           jsonb_typeof(v_event->'entity_type') <> 'string' OR
           jsonb_typeof(v_event->'entity_id') <> 'string' OR
           length(btrim(v_event->>'entity_id')) NOT BETWEEN 1 AND 200 OR
           v_event->>'entity_type' NOT IN ('categories','items','waiters','raw_materials',
             'raw_material_purchases','expenses','sales','sale_reversals',
             'waiter_settlements','material_month_closings') OR
           jsonb_typeof(v_event->'operation') <> 'string' OR v_event->>'operation' <> 'upsert' OR jsonb_typeof(v_event->'payload') <> 'object' THEN
            RAISE EXCEPTION 'unsupported event envelope' USING ERRCODE='22023';
        END IF;
        IF (v_event->'payload') ? 'origin_installation_id' AND
           (jsonb_typeof(v_event->'payload'->'origin_installation_id') <> 'string' OR
            lower(v_event->'payload'->>'origin_installation_id') <> p_installation_id::text) THEN
            RAISE EXCEPTION 'payload origin does not match authenticated installation' USING ERRCODE='42501';
        END IF;
        v_event_id := (v_event->>'event_id')::uuid;
        v_sequence := public.upload_sync_event(p_workspace_id,p_installation_id,v_event_id,
          v_event->>'entity_type',v_event->>'entity_id',v_event->>'operation',v_event->'payload');
        event_id := v_event_id;
        sequence_id := v_sequence;
        RETURN NEXT;
    END LOOP;
END;
$$;
REVOKE ALL ON FUNCTION public.upload_sync_batch(uuid,uuid,uuid,jsonb) FROM PUBLIC;
