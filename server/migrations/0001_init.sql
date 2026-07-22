-- wg-vpng schema: auth (users/orgs/tokens) + WireGuard interfaces & peers.

CREATE TABLE IF NOT EXISTS users (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email      TEXT NOT NULL UNIQUE,
    name       TEXT NOT NULL DEFAULT '',
    is_admin   BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS organizations (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name       TEXT NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS organization_members (
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    user_id         UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role            TEXT NOT NULL DEFAULT 'read',
    PRIMARY KEY (organization_id, user_id)
);

-- API tokens for the plan-ai-api-mcp REST/MCP surface. token_hash is the
-- hex sha256 of the bearer token; kind is 'admin' or 'api'.
CREATE TABLE IF NOT EXISTS tokens (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID REFERENCES organizations(id) ON DELETE CASCADE,
    token_hash      TEXT NOT NULL UNIQUE,
    label           TEXT NOT NULL DEFAULT '',
    kind            TEXT NOT NULL DEFAULT 'api',
    scopes          JSONB,
    revoked         BOOLEAN NOT NULL DEFAULT false,
    expires_at      TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- The managed WireGuard interface(s). Keys are generated + stored on first
-- boot so they survive restarts.
CREATE TABLE IF NOT EXISTS wg_interfaces (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name        TEXT NOT NULL UNIQUE,
    listen_port INTEGER NOT NULL,
    address     TEXT NOT NULL,       -- server tunnel addr, e.g. 10.8.0.1/24
    private_key TEXT NOT NULL,
    public_key  TEXT NOT NULL,
    endpoint    TEXT NOT NULL,       -- public host:port clients dial
    dns         TEXT,
    allowed_ips TEXT NOT NULL DEFAULT '10.8.0.0/24',
    keepalive   INTEGER NOT NULL DEFAULT 25,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- One peer per user (per interface). private_key is the client's; it is stored
-- so the config can be re-shown, and replaced on regeneration.
CREATE TABLE IF NOT EXISTS wg_peers (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    interface_id  UUID NOT NULL REFERENCES wg_interfaces(id) ON DELETE CASCADE,
    user_id       UUID REFERENCES users(id) ON DELETE CASCADE,
    name          TEXT NOT NULL DEFAULT '',
    private_key   TEXT NOT NULL,
    public_key    TEXT NOT NULL,
    preshared_key TEXT,
    address       TEXT NOT NULL,     -- client tunnel addr, e.g. 10.8.0.5/32
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (interface_id, address),
    UNIQUE (interface_id, public_key)
);

CREATE INDEX IF NOT EXISTS wg_peers_interface_idx ON wg_peers(interface_id);
CREATE INDEX IF NOT EXISTS wg_peers_user_idx ON wg_peers(user_id);
