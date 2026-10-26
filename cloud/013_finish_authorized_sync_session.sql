-- Task 38: authenticated terminal session action; apply after 005/006.
CREATE OR REPLACE FUNCTION public.finish_authorized_sync_session(
    p_workspace_id uuid,p_installation_id uuid,p_session_id uuid,p_action text
) RETURNS boolean
LANGUAGE plpgsql
SECURITY INVOKER
SET search_path = pg_catalog, public
AS $$
DECLARE
    v_session public.sync_sessions%ROWTYPE;
BEGIN
    IF p_workspace_id IS NULL OR p_installation_id IS NULL OR p_session_id IS NULL OR
       p_action IS NULL OR p_action NOT IN ('complete','fail') THEN
        RAISE EXCEPTION 'invalid terminal session request' USING ERRCODE='22023';
    END IF;
    -- Same workspace/device/session order as upload and pull.
    PERFORM 1 FROM public.sync_workspaces WHERE id=p_workspace_id FOR SHARE;
    IF NOT FOUND THEN RAISE EXCEPTION 'workspace unavailable' USING ERRCODE='42501'; END IF;
    PERFORM 1 FROM public.sync_devices
     WHERE workspace_id=p_workspace_id AND installation_id=p_installation_id
       AND deactivated_at IS NULL FOR UPDATE;
    IF NOT FOUND THEN RAISE EXCEPTION 'device unavailable' USING ERRCODE='42501'; END IF;
    SELECT * INTO v_session FROM public.sync_sessions
     WHERE id=p_session_id AND installation_id=p_installation_id FOR UPDATE;
    IF NOT FOUND THEN RAISE EXCEPTION 'session unavailable' USING ERRCODE='42501'; END IF;
    IF p_action='complete' THEN
        IF v_session.status='running' AND
           (v_session.started_at>clock_timestamp() OR
            v_session.started_at<=clock_timestamp()-interval '15 minutes') THEN
            PERFORM public.fail_sync_session(p_installation_id,p_session_id);
            RETURN false;
        END IF;
        RETURN public.complete_sync_session(p_installation_id,p_session_id);
    END IF;
    RETURN public.fail_sync_session(p_installation_id,p_session_id);
END;
$$;
REVOKE ALL ON FUNCTION public.finish_authorized_sync_session(uuid,uuid,uuid,text) FROM PUBLIC;
