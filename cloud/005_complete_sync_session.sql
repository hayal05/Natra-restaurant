-- Task 17: complete a sync session only if it is still active and unexpired.
-- PostgreSQL only; apply after cloud/004_reserve_sync_session.sql.
-- Call ONLY from the trusted API after all sync data has been committed.
-- The API must authenticate the device and verify the session belongs to it.
CREATE OR REPLACE FUNCTION complete_sync_session(
    p_installation_id uuid,
    p_session_id uuid
) RETURNS boolean
LANGUAGE plpgsql
AS $$
DECLARE
    v_now timestamptz;
    v_session sync_sessions%ROWTYPE;
    v_day_start timestamptz;
    v_next_day timestamptz;
    v_completed integer;
BEGIN
    IF p_installation_id IS NULL OR p_session_id IS NULL THEN
        RAISE EXCEPTION 'installation and session IDs are required';
    END IF;

    -- Same lock order as reservation; prevents concurrent completions
    -- and reservations from bypassing the device's daily allowance.
    PERFORM 1 FROM sync_devices
     WHERE installation_id = p_installation_id AND deactivated_at IS NULL
     FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'device is not registered or is deactivated';
    END IF;

    SELECT * INTO v_session
      FROM sync_sessions
     WHERE id = p_session_id AND installation_id = p_installation_id
     FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'sync session not found for this installation';
    END IF;

    v_now := clock_timestamp();

    -- A repeated completion is safe: it never consumes another slot.
    IF v_session.status = 'completed' THEN
        RETURN true;
    END IF;
    IF v_session.status <> 'running' THEN
        RETURN false;
    END IF;

    IF v_session.started_at > v_now OR v_session.started_at <= v_now - interval '15 minutes' THEN
        UPDATE sync_sessions SET status = 'failed'
         WHERE id = p_session_id;
        RETURN false;
    END IF;

    v_day_start := date_trunc('day', v_now AT TIME ZONE 'Africa/Addis_Ababa')
                   AT TIME ZONE 'Africa/Addis_Ababa';
    v_next_day := (date_trunc('day', v_now AT TIME ZONE 'Africa/Addis_Ababa')
                   + interval '1 day') AT TIME ZONE 'Africa/Addis_Ababa';

    SELECT count(*)::integer INTO v_completed
      FROM sync_sessions
     WHERE installation_id = p_installation_id
       AND status = 'completed'
       AND completed_at >= v_day_start
       AND completed_at < v_next_day;

    -- Also protects a completion that crosses midnight in EAT.
    IF v_completed >= 10 THEN
        UPDATE sync_sessions SET status = 'failed'
         WHERE id = p_session_id;
        RETURN false;
    END IF;

    UPDATE sync_sessions
       SET status = 'completed', completed_at = v_now
     WHERE id = p_session_id;
    RETURN true;
END;
$$;
