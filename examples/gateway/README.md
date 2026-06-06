# Gateway integration (Kong / Envoy)

Use `auth.type: trustGateway` when an upstream API gateway terminates authentication and forwards trusted identity headers to aiREST.

## aiREST endpoint

```yaml
auth:
  required: true
  type: trustGateway
  trustGateway:
    userIdHeader: x-user-id
    tenantIdHeader: x-tenant-id
```

aiREST reads the configured headers and builds request `AuthContext` (subject, tenant) without validating a bearer token itself.

## Kong

```yaml
# kong.yml fragment
plugins:
  - name: jwt
    config:
      claims_to_verify: [exp]
  - name: request-transformer
    config:
      add:
        headers:
          - "x-user-id:$(jwt.sub)"
          - "x-tenant-id:$(jwt.tenant)"
```

Route traffic to aiREST only from the internal network. Kong should strip any client-supplied `x-user-id` / `x-tenant-id` before adding trusted values.

## Envoy

```yaml
# ext_authz or JWT filter sets headers before routing to aiREST
typed_per_filter_config:
  envoy.filters.http.jwt_authn:
    "@type": type.googleapis.com/envoy.extensions.filters.http.jwt_authn.v3.PerRouteConfig
    requirement_name: airest_jwt
```

Add a Lua or external authorization filter to inject:

- `x-user-id` from JWT `sub`
- `x-tenant-id` from a tenant claim

## OAuth2 introspection (alternative)

For bearer tokens validated at aiREST instead of the gateway, use `auth.type: oauth2Introspect` with `AIREST_OAUTH2_INTROSPECTION_URL` (RFC 7662). See `.env.example`.
