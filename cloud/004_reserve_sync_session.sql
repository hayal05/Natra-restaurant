-- Task 16: atomically reserve one sync session per registered installation.
-- Apply after cloud/003_sync_sessions.sql. PostgreSQL only.
-- Invoke ONLY from a trusted API after device/workspace authentication.
-- Completion, retry tokens and API authentication are separate tasks.
CREATE OR REPLACE FUNCTION reserve_sync_session(
    p_installation_id uuid,
    p_session_id uuid
) RETURNS uuid
LANGUAGE plpgsql
AS $$
DECLARE
    v_day_start timestamptz;
    v_next_day timestamptz;
    v_used integer;
BEGIN
    IF p_installation_id IS NULL OR p_session_id IS NULL THEN
        RAISE EXCEPTION 'installation and session IDs are required';
    END IF;

    -- Serializes reservations (including across concurrent API instances).
    PERFORM 1 FROM sync_devices
    WHERE installation_id = p_installation_id AND deactivated_at IS NULL
    FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'device is not registered or is deactivated';
    END IF;

    -- Abandoned sessions expire after 15 minutes; a later completion
    -- implementation must only complete sessions still marked running.
    UPDATE sync_sessions
       SET status = 'failed'
     WHERE installation_id = p_installation_id
       AND status = 'running'
       AND started_at < now() - interval '15 minutes';

    -- Do not permit two active syncs on one installation.
    IF EXISTS (
        SELECT 1 FROM sync_sessions
         WHERE installation_id = p_installation_id AND status = 'running'
    ) THEN
        RAISE EXCEPTION 'synchronization already in progress';
    END IF;

    v_day_start := date_trunc('day', now() AT TIME ZONE 'Africa/Addis_Ababa')
                   AT TIME ZONE 'Africa/Addis_Ababa';
    v_next_day := (date_trunc('day', now() AT TIME ZONE 'Africa/Addis_Ababa')
                   + interval '1 day') AT TIME ZONE 'Africa/Addis_Ababa';

    SELECT count(*)::integer INTO v_used
      FROM sync_sessions
     WHERE installation_id = p_installation_id
       AND ((status = 'completed' AND completed_at >= v_day_start
             AND completed_at < v_next_day)
            OR (status = 'running' AND started_at >= v_day_start
                AND started_at < v_next_day));

    IF v_used >= 10 THEN
        RAISE EXCEPTION 'daily sync limit reached (10 per installation)';
    END IF;

    INSERT INTO sync_sessions(id, installation_id, status)
    VALUES (p_session_id, p_installation_id, 'running');
    RETURN p_session_id;
END;
$$;
