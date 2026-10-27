-- Post-audit source hardening. Apply only during an explicitly authorized
-- deployment, after 001–014, as the function owner. Not a SQLite migration.
-- Keep trusted schemas before pg_temp so temporary relations cannot shadow
-- the legacy helpers' unqualified table names. public must be owner-controlled.
-- This changes no financial data or function bodies and grants no new access.
BEGIN;

ALTER FUNCTION public.reserve_sync_session(uuid,uuid) SECURITY INVOKER;
ALTER FUNCTION public.reserve_sync_session(uuid,uuid) SET search_path = pg_catalog, public, pg_temp;
REVOKE ALL ON FUNCTION public.reserve_sync_session(uuid,uuid) FROM PUBLIC;

ALTER FUNCTION public.complete_sync_session(uuid,uuid) SECURITY INVOKER;
ALTER FUNCTION public.complete_sync_session(uuid,uuid) SET search_path = pg_catalog, public, pg_temp;
REVOKE ALL ON FUNCTION public.complete_sync_session(uuid,uuid) FROM PUBLIC;

ALTER FUNCTION public.fail_sync_session(uuid,uuid) SECURITY INVOKER;
ALTER FUNCTION public.fail_sync_session(uuid,uuid) SET search_path = pg_catalog, public, pg_temp;
REVOKE ALL ON FUNCTION public.fail_sync_session(uuid,uuid) FROM PUBLIC;

ALTER FUNCTION public.upload_sync_event(uuid,uuid,uuid,text,text,text,jsonb) SECURITY INVOKER;
ALTER FUNCTION public.upload_sync_event(uuid,uuid,uuid,text,text,text,jsonb) SET search_path = pg_catalog, public, pg_temp;
REVOKE ALL ON FUNCTION public.upload_sync_event(uuid,uuid,uuid,text,text,text,jsonb) FROM PUBLIC;

ALTER FUNCTION public.enroll_sync_device(uuid,uuid) SECURITY INVOKER;
ALTER FUNCTION public.enroll_sync_device(uuid,uuid) SET search_path = pg_catalog, public, pg_temp;
REVOKE ALL ON FUNCTION public.enroll_sync_device(uuid,uuid) FROM PUBLIC;

ALTER FUNCTION public.reserve_authorized_sync_session(uuid,uuid,uuid) SECURITY INVOKER;
ALTER FUNCTION public.reserve_authorized_sync_session(uuid,uuid,uuid) SET search_path = pg_catalog, public, pg_temp;
REVOKE ALL ON FUNCTION public.reserve_authorized_sync_session(uuid,uuid,uuid) FROM PUBLIC;

ALTER FUNCTION public.upload_sync_batch(uuid,uuid,uuid,jsonb) SECURITY INVOKER;
ALTER FUNCTION public.upload_sync_batch(uuid,uuid,uuid,jsonb) SET search_path = pg_catalog, public, pg_temp;
REVOKE ALL ON FUNCTION public.upload_sync_batch(uuid,uuid,uuid,jsonb) FROM PUBLIC;

ALTER FUNCTION public.pull_authorized_sync_events(uuid,uuid,uuid,bigint,integer) SECURITY INVOKER;
ALTER FUNCTION public.pull_authorized_sync_events(uuid,uuid,uuid,bigint,integer) SET search_path = pg_catalog, public, pg_temp;
REVOKE ALL ON FUNCTION public.pull_authorized_sync_events(uuid,uuid,uuid,bigint,integer) FROM PUBLIC;

ALTER FUNCTION public.finish_authorized_sync_session(uuid,uuid,uuid,text) SECURITY INVOKER;
ALTER FUNCTION public.finish_authorized_sync_session(uuid,uuid,uuid,text) SET search_path = pg_catalog, public, pg_temp;
REVOKE ALL ON FUNCTION public.finish_authorized_sync_session(uuid,uuid,uuid,text) FROM PUBLIC;

ALTER FUNCTION public.sync_day_bounds(timestamptz) SECURITY INVOKER;
ALTER FUNCTION public.sync_day_bounds(timestamptz) SET search_path = pg_catalog, public, pg_temp;
REVOKE ALL ON FUNCTION public.sync_day_bounds(timestamptz) FROM PUBLIC;

ALTER FUNCTION public.authorized_sync_allowance(uuid,uuid,uuid) SECURITY INVOKER;
ALTER FUNCTION public.authorized_sync_allowance(uuid,uuid,uuid) SET search_path = pg_catalog, public, pg_temp;
REVOKE ALL ON FUNCTION public.authorized_sync_allowance(uuid,uuid,uuid) FROM PUBLIC;

COMMIT;
