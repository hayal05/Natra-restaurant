-- Trusted issuer source only. Apply after 001–015 during an authorized
-- deployment. A credential is random bearer material; store only its digest.
BEGIN;
CREATE TABLE public.sync_credentials (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES public.sync_workspaces(id),
    installation_id uuid NOT NULL REFERENCES public.sync_devices(installation_id),
    local_user_id bigint NOT NULL CHECK (local_user_id > 0),
    subject text NOT NULL CHECK (length(btrim(subject)) BETWEEN 1 AND 256),
    secret_digest text NOT NULL UNIQUE CHECK (length(secret_digest)=64 AND secret_digest ~ '^[0-9a-f]{64}$'),
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    expires_at timestamptz NOT NULL DEFAULT clock_timestamp()+interval '30 days',
    revoked_at timestamptz,
    CHECK (expires_at > created_at),
    FOREIGN KEY (workspace_id,installation_id)
      REFERENCES public.sync_devices(workspace_id,installation_id)
);
CREATE UNIQUE INDEX sync_credentials_active_user ON public.sync_credentials(installation_id,local_user_id)
    WHERE revoked_at IS NULL;
CREATE TABLE public.sync_credential_sessions (
    session_id uuid PRIMARY KEY REFERENCES public.sync_sessions(id),
    credential_id uuid NOT NULL REFERENCES public.sync_credentials(id)
);
CREATE INDEX sync_credential_sessions_credential ON public.sync_credential_sessions(credential_id);
REVOKE ALL ON public.sync_credentials,public.sync_credential_sessions FROM PUBLIC;

CREATE FUNCTION public.register_sync_credential(
    p_id uuid,p_workspace uuid,p_installation uuid,p_local_user bigint,p_subject text,p_digest text
) RETURNS uuid LANGUAGE plpgsql SECURITY INVOKER
SET search_path=pg_catalog,public,pg_temp AS $$
BEGIN
    IF p_id IS NULL OR p_workspace IS NULL OR p_installation IS NULL OR p_local_user IS NULL OR p_local_user<=0
       OR p_subject IS NULL OR length(btrim(p_subject)) NOT BETWEEN 1 AND 256
       OR p_digest IS NULL OR length(p_digest)<>64 OR p_digest !~ '^[0-9a-f]{64}$' THEN
      RAISE EXCEPTION 'invalid credential registration' USING ERRCODE='22023'; END IF;
    PERFORM 1 FROM public.sync_workspaces WHERE id=p_workspace FOR SHARE;
    IF NOT FOUND THEN RAISE EXCEPTION 'scope unavailable' USING ERRCODE='42501'; END IF;
    PERFORM 1 FROM public.sync_devices WHERE workspace_id=p_workspace AND installation_id=p_installation
      AND deactivated_at IS NULL FOR UPDATE;
    IF NOT FOUND THEN RAISE EXCEPTION 'scope unavailable' USING ERRCODE='42501'; END IF;
    INSERT INTO public.sync_credentials(id,workspace_id,installation_id,local_user_id,subject,secret_digest)
      VALUES(p_id,p_workspace,p_installation,p_local_user,p_subject,p_digest);
    RETURN p_id;
END $$;
REVOKE ALL ON FUNCTION public.register_sync_credential(uuid,uuid,uuid,bigint,text,text) FROM PUBLIC;

CREATE FUNCTION public.reserve_credential_sync_session(
    p_digest text,p_installation uuid,p_local_user bigint,p_session uuid
) RETURNS TABLE(credential_id uuid,workspace_id uuid,installation_id uuid,local_user_id text,subject text,
    session_id uuid,started_at timestamptz,expires_at timestamptz)
LANGUAGE plpgsql SECURITY INVOKER SET search_path=pg_catalog,public,pg_temp AS $$
DECLARE v_grant public.sync_credentials%ROWTYPE; v_workspace uuid;
BEGIN
    IF p_digest IS NULL OR length(p_digest)<>64 OR p_digest !~ '^[0-9a-f]{64}$' OR p_installation IS NULL
       OR p_local_user IS NULL OR p_local_user<=0 OR p_session IS NULL THEN
      RAISE EXCEPTION 'credential unavailable' USING ERRCODE='42501'; END IF;
    -- Lookup supplies scope only. Re-read under locks after taking the shared
    -- workspace/device order used by upload, completion and credential revocation.
    SELECT * INTO v_grant FROM public.sync_credentials c WHERE c.secret_digest=p_digest
      AND c.installation_id=p_installation AND c.local_user_id=p_local_user;
    IF NOT FOUND THEN RAISE EXCEPTION 'credential unavailable' USING ERRCODE='42501'; END IF;
    v_workspace := v_grant.workspace_id;
    PERFORM 1 FROM public.sync_workspaces WHERE id=v_workspace FOR SHARE;
    IF NOT FOUND THEN RAISE EXCEPTION 'credential unavailable' USING ERRCODE='42501'; END IF;
    PERFORM 1 FROM public.sync_devices d WHERE d.installation_id=p_installation
      AND d.workspace_id=v_workspace AND d.deactivated_at IS NULL FOR UPDATE;
    IF NOT FOUND THEN RAISE EXCEPTION 'credential unavailable' USING ERRCODE='42501'; END IF;
    SELECT * INTO v_grant FROM public.sync_credentials c WHERE c.id=v_grant.id
      AND c.workspace_id=v_workspace AND c.installation_id=p_installation
      AND c.local_user_id=p_local_user AND c.secret_digest=p_digest FOR UPDATE;
    IF NOT FOUND THEN RAISE EXCEPTION 'credential unavailable' USING ERRCODE='42501'; END IF;
    IF v_grant.revoked_at IS NOT NULL OR v_grant.created_at>clock_timestamp()
       OR v_grant.expires_at<=clock_timestamp() THEN
      RAISE EXCEPTION 'credential unavailable' USING ERRCODE='42501'; END IF;
    PERFORM public.reserve_authorized_sync_session(v_grant.workspace_id,p_installation,p_session);
    INSERT INTO public.sync_credential_sessions(session_id,credential_id) VALUES(p_session,v_grant.id);
    RETURN QUERY SELECT v_grant.id,v_grant.workspace_id,p_installation,p_local_user::text,v_grant.subject,
      s.id,s.started_at,least(s.started_at+interval '15 minutes',v_grant.expires_at)
      FROM public.sync_sessions s WHERE s.id=p_session AND s.installation_id=p_installation;
END $$;
REVOKE ALL ON FUNCTION public.reserve_credential_sync_session(text,uuid,bigint,uuid) FROM PUBLIC;

CREATE FUNCTION public.revoke_sync_credential(p_workspace uuid,p_installation uuid,p_id uuid)
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
    -- Existing operation locks serialize with this revocation. Completed
    -- sessions/accounting remain intact; only running mapped sessions fail.
    UPDATE public.sync_sessions s SET status='failed' WHERE s.status='running' AND s.id IN
      (SELECT m.session_id FROM public.sync_credential_sessions m WHERE m.credential_id=p_id);
    RETURN true;
END $$;
REVOKE ALL ON FUNCTION public.revoke_sync_credential(uuid,uuid,uuid) FROM PUBLIC;
COMMIT;
