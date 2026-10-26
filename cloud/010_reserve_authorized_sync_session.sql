-- Task 35: reserve only within the trusted issuer's authorized workspace.
-- PostgreSQL, after 004_reserve_sync_session.sql. Not a SQLite migration.
CREATE OR REPLACE FUNCTION public.reserve_authorized_sync_session(
    p_workspace_id uuid,
    p_installation_id uuid,
    p_session_id uuid
) RETURNS TABLE(session_id uuid, started_at timestamptz, expires_at timestamptz)
LANGUAGE plpgsql
SECURITY INVOKER
SET search_path = pg_catalog, public
AS $$
DECLARE
    v_session_id uuid;
BEGIN
    IF p_workspace_id IS NULL OR p_installation_id IS NULL OR p_session_id IS NULL THEN
        RAISE EXCEPTION 'workspace, installation and session IDs are required';
    END IF;

    -- Keep this lock until reservation commits. Scope cannot change between
    -- this authorization check and the existing quota/session enforcement.
    PERFORM 1 FROM public.sync_devices
     WHERE workspace_id = p_workspace_id
       AND installation_id = p_installation_id
       AND deactivated_at IS NULL
     FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'installation is not active in the authorized workspace';
    END IF;

    v_session_id := public.reserve_sync_session(p_installation_id, p_session_id);
    RETURN QUERY
      SELECT s.id, s.started_at, s.started_at + interval '15 minutes'
        FROM public.sync_sessions AS s
       WHERE s.id = v_session_id AND s.installation_id = p_installation_id
         AND s.status = 'running';
END;
$$;
REVOKE ALL ON FUNCTION public.reserve_authorized_sync_session(uuid, uuid, uuid) FROM PUBLIC;
