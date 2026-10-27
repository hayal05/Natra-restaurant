-- Source only. Apply after 001–016 in a separately authorized environment.
-- Quiesce issuer, revocation and data traffic during migration/backfill.
-- Revoke every mapped bearer session, including terminal retries/metadata,
-- without granting the data runtime access to credential digests or subjects.
BEGIN;
CREATE TABLE public.sync_revoked_sessions (
    session_id uuid PRIMARY KEY REFERENCES public.sync_sessions(id),
    revoked_at timestamptz NOT NULL
);
REVOKE ALL ON public.sync_revoked_sessions FROM PUBLIC;
-- Cover credentials revoked before this migration, without rewriting history.
INSERT INTO public.sync_revoked_sessions(session_id,revoked_at)
SELECT m.session_id,c.revoked_at FROM public.sync_credential_sessions m
JOIN public.sync_credentials c ON c.id=m.credential_id WHERE c.revoked_at IS NOT NULL;

CREATE FUNCTION public.assert_sync_session_not_revoked(p_session_id uuid)
RETURNS void LANGUAGE plpgsql SECURITY INVOKER
SET search_path=pg_catalog,public,pg_temp AS $$
BEGIN
    IF p_session_id IS NULL OR EXISTS (
        SELECT 1 FROM public.sync_revoked_sessions WHERE session_id=p_session_id
    ) THEN RAISE EXCEPTION 'session unavailable' USING ERRCODE='42501'; END IF;
END $$;
REVOKE ALL ON FUNCTION public.assert_sync_session_not_revoked(uuid) FROM PUBLIC;

CREATE OR REPLACE FUNCTION public.revoke_sync_credential(p_workspace uuid,p_installation uuid,p_id uuid)
RETURNS boolean LANGUAGE plpgsql SECURITY INVOKER SET search_path=pg_catalog,public,pg_temp AS $$
BEGIN
    PERFORM 1 FROM public.sync_workspaces WHERE id=p_workspace FOR SHARE;
    IF NOT FOUND THEN RAISE EXCEPTION 'scope unavailable' USING ERRCODE='42501'; END IF;
    PERFORM 1 FROM public.sync_devices WHERE installation_id=p_installation AND workspace_id=p_workspace FOR UPDATE;
    IF NOT FOUND THEN RAISE EXCEPTION 'scope unavailable' USING ERRCODE='42501'; END IF;
    PERFORM 1 FROM public.sync_credentials c WHERE c.id=p_id AND c.workspace_id=p_workspace
      AND c.installation_id=p_installation FOR UPDATE;
    IF NOT FOUND THEN RAISE EXCEPTION 'credential unavailable' USING ERRCODE='42501'; END IF;
    UPDATE public.sync_credentials SET revoked_at=coalesce(revoked_at,clock_timestamp()) WHERE id=p_id;
    INSERT INTO public.sync_revoked_sessions(session_id,revoked_at)
      SELECT m.session_id,c.revoked_at FROM public.sync_credential_sessions m
      JOIN public.sync_credentials c ON c.id=m.credential_id WHERE c.id=p_id
      ON CONFLICT (session_id) DO NOTHING;
    -- Existing operation locks serialize with this revocation. Completed
    -- sessions/accounting remain intact; only running mapped sessions fail.
    UPDATE public.sync_sessions s SET status='failed' WHERE s.status='running' AND s.id IN
      (SELECT m.session_id FROM public.sync_credential_sessions m WHERE m.credential_id=p_id);
    RETURN true;
END $$;
REVOKE ALL ON FUNCTION public.revoke_sync_credential(uuid,uuid,uuid) FROM PUBLIC;

CREATE OR REPLACE FUNCTION public.upload_sync_batch(
    p_workspace_id uuid, p_installation_id uuid, p_session_id uuid, p_events jsonb
) RETURNS TABLE(event_id uuid, sequence_id bigint)
LANGUAGE plpgsql
SECURITY INVOKER
SET search_path = pg_catalog, public, pg_temp
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
    -- jsonb canonical text adds spaces; allow 2 MiB internally while the
    -- HTTP boundary caps original request bytes at 1 MiB.
    IF jsonb_array_length(p_events) NOT BETWEEN 1 AND 500 OR
       octet_length(p_events::text) > 2097152 THEN
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
    PERFORM public.assert_sync_session_not_revoked(p_session_id);
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

CREATE OR REPLACE FUNCTION public.pull_authorized_sync_events(
    p_workspace_id uuid, p_installation_id uuid, p_session_id uuid,
    p_after bigint, p_limit integer
) RETURNS TABLE(event_id uuid, sequence_id bigint, origin_installation_id uuid,
    entity_type text, entity_id text, operation text, payload jsonb, is_lookahead boolean)
LANGUAGE plpgsql
SECURITY INVOKER
SET search_path = pg_catalog, public, pg_temp
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
    PERFORM public.assert_sync_session_not_revoked(p_session_id);
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

CREATE OR REPLACE FUNCTION public.finish_authorized_sync_session(
    p_workspace_id uuid,p_installation_id uuid,p_session_id uuid,p_action text
) RETURNS boolean
LANGUAGE plpgsql
SECURITY INVOKER
SET search_path = pg_catalog, public, pg_temp
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
    PERFORM public.assert_sync_session_not_revoked(p_session_id);
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

CREATE OR REPLACE FUNCTION public.authorized_sync_allowance(
  p_workspace_id uuid,p_installation_id uuid,p_session_id uuid
) RETURNS TABLE(daily_limit integer,completed_today integer,reserved_today integer,
  remaining_today integer,session_permitted boolean,day text,reset_at timestamptz)
LANGUAGE plpgsql SECURITY INVOKER SET search_path = pg_catalog, public, pg_temp
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
  PERFORM public.assert_sync_session_not_revoked(p_session_id);
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

COMMIT;
