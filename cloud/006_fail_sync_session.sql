-- Task 18: fail an unsuccessful synchronization without consuming daily quota.
-- PostgreSQL only; apply after cloud/005_complete_sync_session.sql.
-- Trusted API must authenticate the installation and its session first.
CREATE OR REPLACE FUNCTION fail_sync_session(
    p_installation_id uuid,
    p_session_id uuid
) RETURNS boolean
LANGUAGE plpgsql
AS $$
DECLARE
    v_status text;
BEGIN
    IF p_installation_id IS NULL OR p_session_id IS NULL THEN
        RAISE EXCEPTION 'installation and session IDs are required';
    END IF;

    -- Keep the same lock order as reserve and complete operations.
    PERFORM 1 FROM sync_devices
     WHERE installation_id = p_installation_id
     FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'device is not registered';
    END IF;

    SELECT status INTO v_status
      FROM sync_sessions
     WHERE id = p_session_id AND installation_id = p_installation_id
     FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'sync session not found for this installation';
    END IF;

    -- Never reverse completed accounting for daily sync usage.
    IF v_status = 'completed' THEN
        RETURN false;
    END IF;

    -- Repeated failure notifications are idempotent.
    IF v_status = 'failed' THEN
        RETURN true;
    END IF;

    UPDATE sync_sessions
       SET status = 'failed'
     WHERE id = p_session_id AND status = 'running';
    RETURN true;
END;
$$;
