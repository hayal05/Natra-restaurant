-- Task 37: bounded incremental read under current authenticated session scope.
-- PostgreSQL only, after 007. Apply only during an authorized cloud deployment.
CREATE OR REPLACE FUNCTION public.pull_authorized_sync_events(
    p_workspace_id uuid, p_installation_id uuid, p_session_id uuid,
    p_after bigint, p_limit integer
) RETURNS TABLE(event_id uuid, sequence_id bigint, origin_installation_id uuid,
    entity_type text, entity_id text, operation text, payload jsonb, is_lookahead boolean)
LANGUAGE plpgsql
SECURITY INVOKER
SET search_path = pg_catalog, public
AS $$
DECLARE
    v_session public.sync_sessions%ROWTYPE;
    v_event public.sync_events%ROWTYPE;
    v_count integer := 0;
    v_bytes bigint := 0;
BEGIN
    IF p_after IS NULL OR p_after < 0 OR p_limit IS NULL OR p_limit NOT BETWEEN 1 AND 500 THEN
        RAISE EXCEPTION 'invalid pull cursor or page limit' USING ERRCODE='22023';
    END IF;
    IF p_workspace_id IS NULL OR p_installation_id IS NULL OR p_session_id IS NULL THEN
        RAISE EXCEPTION 'authenticated scope required' USING ERRCODE='42501';
    END IF;
    -- Same workspace/device/session order as upload; keep scope valid for
    -- the full read and serialize with workspace event writers.
    PERFORM 1 FROM public.sync_workspaces WHERE id=p_workspace_id FOR SHARE;
    IF NOT FOUND THEN RAISE EXCEPTION 'workspace unavailable' USING ERRCODE='42501'; END IF;
    PERFORM 1 FROM public.sync_devices
     WHERE workspace_id=p_workspace_id AND installation_id=p_installation_id
       AND deactivated_at IS NULL FOR SHARE;
    IF NOT FOUND THEN RAISE EXCEPTION 'device unavailable' USING ERRCODE='42501'; END IF;
    SELECT * INTO v_session FROM public.sync_sessions
     WHERE id=p_session_id AND installation_id=p_installation_id FOR SHARE;
    IF NOT FOUND THEN RAISE EXCEPTION 'session unavailable' USING ERRCODE='42501'; END IF;
    IF v_session.status <> 'running' OR v_session.completed_at IS NOT NULL OR
       v_session.started_at > clock_timestamp() OR
       v_session.started_at <= clock_timestamp()-interval '15 minutes' THEN
        RAISE EXCEPTION 'session unavailable' USING ERRCODE='42501';
    END IF;

    FOR v_event IN SELECT e.* FROM public.sync_events AS e
      WHERE e.workspace_id=p_workspace_id AND e.sequence_id>p_after
      ORDER BY e.sequence_id LIMIT p_limit+1 LOOP
        v_count := v_count+1;
        v_bytes := v_bytes+octet_length(v_event.payload::text)
          +octet_length(v_event.entity_type)+octet_length(v_event.entity_id)+512;
        event_id := v_event.event_id;
        sequence_id := v_event.sequence_id;
        origin_installation_id := v_event.origin_installation_id;
        entity_type := v_event.entity_type;
        entity_id := v_event.entity_id;
        operation := v_event.operation;
        payload := v_event.payload;
        -- One sentinel proves there is another event without skipping it.
        is_lookahead := v_count>p_limit OR v_bytes>2097152;
        RETURN NEXT;
        IF is_lookahead THEN EXIT; END IF;
    END LOOP;
END;
$$;
REVOKE ALL ON FUNCTION public.pull_authorized_sync_events(uuid,uuid,uuid,bigint,integer) FROM PUBLIC;
