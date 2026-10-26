# Task 35 authentication integration

Status: implemented for review; not configured or deployed. Upload and pull
still return HTTP 501 after successful authentication and session validation.

The Worker can verify RS256 JWTs using a trusted issuer's public JWKS. This
provides a configurable verification boundary; it does not enroll devices,
create users, issue tokens, or choose an identity provider.

Server configuration, to be supplied only when deployment is authorized:

| Variable | Purpose |
| --- | --- |
| `SYNC_JWT_ISSUER` | Exact expected issuer; HTTPS URL |
| `SYNC_JWT_AUDIENCE` | Audience dedicated to this sync API |
| `SYNC_JWKS_URL` | Pinned HTTPS public-key endpoint |
| `NEON_DATABASE_URL` | Server-only Neon connection secret |

The trusted issuer must authorize the workspace and registered installation
before signing claims. Required claims are `iss`, `aud`, `exp`, a nonempty
`sub`, UUID `workspace_id`, UUID `installation_id`, UUID `session_id`, and a
space-separated `scope` containing `natra:sync`. Normal local restaurant
logins and installation IDs alone do not grant cloud authorization.

The verifier checks the signature, issuer, audience, expiration and `nbf`
when present. It accepts only RS256. Key retrieval uses the configured URL,
not a token's `jku` or `x5u` header, with a five-second timeout and cached keys
that can refresh for key rotation. Authentication errors return generic 401
responses. Missing or invalid server configuration returns 503.

After authentication, the API validates UUID scope and queries current Neon
state: the device must remain active in the claimed workspace and its session
must remain running and unexpired. Future financial operations must repeat
that database authorization within their own transaction; this preflight's
locks do not survive its HTTP query.

The function-based `SYNC_AUTH_VERIFIER` and `SYNC_SESSION_VALIDATOR` overrides
remain available for trusted server composition and tests; request fields
cannot set them. Production selection/configuration of an issuer and its
secure enrollment/token-issuance flow remains pending. Tests use generated
keys and mocked HTTP; no live identity provider or Neon database was contacted.
