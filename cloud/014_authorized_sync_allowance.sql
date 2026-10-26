-- Task 39: authoritative EAT calendar day and scoped allowance reporting.
CREATE OR REPLACE FUNCTION public.sync_day_bounds(p_at timestamptz)
RETURNS TABLE(day_start timestamptz,next_day_start timestamptz)
LANGUAGE sql IMMUTABLE STRICT
SET search_path = pg_catalog, public
AS $$ SELECT date_trunc('day',p_at AT TIME ZONE 'Africa/Addis_Ababa') AT TIME ZONE 'Africa/Addis_Ababa',
    (date_trunc('day',p_at AT TIME ZONE 'Africa/Addis_Ababa')+interval '1 day') AT TIME ZONE 'Africa/Addis_Ababa' $$;

CREATE OR REPLACE FUNCTION public.authorized_sync_allowance(
  p_workspace_id uuid,p_installation_id uuid,p_session_id uuid
) RETURNS TABLE(daily_limit integer,completed_today integer,reserved_today integer,
  remaining_today integer,session_permitted boolean,day text,reset_at timestamptz)
LANGUAGE plpgsql SECURITY INVOKER SET search_path = pg_catalog, public
AS $$
DECLARE
  v_session public.sync_sessions%ROWTYPE;
  v_now timestamptz;
  v_start timestamptz;
  v_end timestamptz;
  v_completed integer;
  v_reserved integer;
BEGIN
  IF p_workspace_id IS NULL OR p_installation_id IS NULL OR p_session_id IS NULL THEN
    RAISE EXCEPTION 'scope required' USING ERRCODE='42501'; END IF;
  PERFORM 1 FROM public.sync_workspaces WHERE id=p_workspace_id FOR SHARE;
  IF NOT FOUND THEN RAISE EXCEPTION 'workspace unavailable' USING ERRCODE='42501'; END IF;
  PERFORM 1 FROM public.sync_devices WHERE workspace_id=p_workspace_id AND installation_id=p_installation_id
    AND deactivated_at IS NULL FOR SHARE;
  IF NOT FOUND THEN RAISE EXCEPTION 'device unavailable' USING ERRCODE='42501'; END IF;
  SELECT * INTO v_session FROM public.sync_sessions WHERE id=p_session_id AND installation_id=p_installation_id FOR SHARE;
  IF NOT FOUND THEN RAISE EXCEPTION 'session unavailable' USING ERRCODE='42501'; END IF;
  v_now := clock_timestamp();
  SELECT b.day_start,b.next_day_start INTO v_start,v_end FROM public.sync_day_bounds(v_now) b;
  SELECT count(*)::integer INTO v_completed FROM public.sync_sessions s WHERE s.installation_id=p_installation_id
    AND s.status='completed' AND s.completed_at>=v_start AND s.completed_at<v_end;
  -- A still-active session spanning midnight also reserves its one slot.
  SELECT count(*)::integer INTO v_reserved FROM public.sync_sessions s WHERE s.installation_id=p_installation_id
    AND s.status='running' AND s.started_at>v_now-interval '15 minutes' AND s.started_at<=v_now;
  RETURN QUERY SELECT 10,v_completed,v_reserved,greatest(0,10-v_completed-v_reserved),
    v_session.status='running' AND v_session.completed_at IS NULL AND v_session.started_at<=v_now
      AND v_session.started_at>v_now-interval '15 minutes' AND v_completed<10 AND v_reserved<=1,
    to_char(v_now AT TIME ZONE 'Africa/Addis_Ababa','YYYY-MM-DD'),v_end;
END;
$$;
REVOKE ALL ON FUNCTION public.authorized_sync_allowance(uuid,uuid,uuid) FROM PUBLIC;
