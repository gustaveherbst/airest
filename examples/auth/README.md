# Authentication examples

Runnable YAML samples for non–`apiKey` auth strategies. Copy into your `api/` directory or load this folder for local experiments:

```bash
airest serve --env-file ./.env --folder ./examples/auth
```

| File | `auth.type` | Caller credential |
|------|-------------|-------------------|
| `jwt-protected.yaml` | `jwt` | `Authorization: Bearer <RS256 JWT>` |
| `oauth2-introspect.yaml` | `oauth2Introspect` | `Authorization: Bearer <access token>` |
| `trust-gateway.yaml` | `trustGateway` | `x-user-id` + optional `x-tenant-id` (gateway injects) |

## Environment overrides

Global defaults from `.env` (or `--env-file`) apply when YAML omits URLs:

```bash
AIREST_JWT_JWKS_URL=https://auth.example.com/.well-known/jwks.json
AIREST_JWT_ISSUER=https://auth.example.com
AIREST_JWT_AUDIENCE=airest-api
AIREST_OAUTH2_INTROSPECTION_URL=https://auth.example.com/oauth/introspect
AIREST_OAUTH2_CLIENT_ID=...
AIREST_OAUTH2_CLIENT_SECRET=...

# Optional — for ./examples/runall.sh auth smoke tests
AIREST_TEST_JWT=<valid RS256 JWT>
AIREST_TEST_OAUTH2_TOKEN=<valid access token>
```

Without `AIREST_TEST_JWT` / `AIREST_TEST_OAUTH2_TOKEN`, `runall.sh` skips the JWT and OAuth2 endpoints.

## Gateway pattern

See [`../gateway/README.md`](../gateway/README.md) for Kong/Envoy snippets that terminate JWT/OAuth and forward trusted headers to `trust-gateway.yaml`.

## OpenAPI

`GET /openapi.json` emits `securitySchemes` matching each endpoint's `auth.type`.
