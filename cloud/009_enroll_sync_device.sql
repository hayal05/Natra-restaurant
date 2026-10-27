-- Task 35: trusted-administrator enrollment only. PostgreSQL, after 002.
-- Do not expose this function to an unauthenticated request. The caller must
-- authorize administration of the workspace before invoking this function.
CREATE OR REPLACE FUNCTION public.enroll_sync_device(
    p_workspace_id uuid,
    p_installation_id uuid
) RETURNS uuid
LANGUAGE plpgsql
SECURITY INVOKER
SET search_path = pg_catalog, public
AS $$
DECLARE
    v_device public.sync_devices%ROWTYPE;
BEGIN
    IF p_workspace_id IS NULL OR p_installation_id IS NULL THEN
        RAISE EXCEPTION 'workspace and installation IDs are required';
    END IF;

    PERFORM 1 FROM public.sync_workspaces
     WHERE id = p_workspace_id FOR KEY SHARE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'workspace not found';
    END IF;

    -- Concurrent duplicate registrations cannot create two bindings.
    INSERT INTO public.sync_devices(installation_id, workspace_id)
    VALUES (p_installation_id, p_workspace_id)
    ON CONFLICT (installation_id) DO NOTHING;

    SELECT * INTO STRICT v_device FROM public.sync_devices
     WHERE installation_id = p_installation_id FOR UPDATE;
    IF v_device.workspace_id <> p_workspace_id THEN
        RAISE EXCEPTION 'installation already belongs to another workspace';
    END IF;
    IF v_device.deactivated_at IS NOT NULL THEN
        RAISE EXCEPTION 'installation is deactivated; separate approval is required';
    END IF;
    RETURN v_device.installation_id;
END;
$$;

-- The owner can invoke it; grant execution only to a trusted provisioning
-- role if a separate runtime role is introduced. No SECURITY DEFINER powers.
REVOKE ALL ON FUNCTION public.enroll_sync_device(uuid, uuid) FROM PUBLIC;
