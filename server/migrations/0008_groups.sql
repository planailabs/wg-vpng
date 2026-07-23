-- Groups: reusable named sets of access rules (email patterns + OIDC claim
-- values). Interfaces reference groups instead of carrying inline patterns, and
-- group membership can come from an email match or an OIDC group claim.
CREATE TABLE IF NOT EXISTS wg_groups (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name         TEXT NOT NULL UNIQUE,
    patterns     JSONB NOT NULL DEFAULT '[]'::jsonb,  -- email globs / literal emails; * = everyone
    claim_values JSONB NOT NULL DEFAULT '[]'::jsonb,  -- matched against a user's OIDC group claim
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Interfaces reference groups; access = union of the assigned groups' members.
ALTER TABLE wg_interfaces ADD COLUMN IF NOT EXISTS group_ids JSONB NOT NULL DEFAULT '[]'::jsonb;

-- Migrate each interface's inline patterns into a group named after the
-- interface, then point the interface at that group.
INSERT INTO wg_groups (name, patterns)
SELECT name, access_patterns FROM wg_interfaces
WHERE jsonb_array_length(access_patterns) > 0
ON CONFLICT (name) DO NOTHING;

UPDATE wg_interfaces i
SET group_ids = jsonb_build_array((SELECT g.id FROM wg_groups g WHERE g.name = i.name))
WHERE jsonb_array_length(i.access_patterns) > 0;

ALTER TABLE wg_interfaces DROP COLUMN access_patterns;

-- Devices: track whether the end user created the device (vs an admin). Users
-- may rename only their own (user-created) devices.
ALTER TABLE wg_peers ADD COLUMN IF NOT EXISTS user_created BOOLEAN NOT NULL DEFAULT false;

-- Captured OIDC group claims, per user per provider. A user's effective claim
-- groups are the union across providers; re-login via a provider refreshes
-- exactly that provider's set (so IdP-side removals revoke on next login).
CREATE TABLE IF NOT EXISTS user_provider_groups (
    user_id  UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    groups   JSONB NOT NULL DEFAULT '[]'::jsonb,
    PRIMARY KEY (user_id, provider)
);

-- Liveliness: a user must log in within the provider's configured TTL (default
-- 30d) or their account deactivates (devices drop until they log in again).
-- `live_until` is set to now() + TTL at each login; deactivated = it has passed.
ALTER TABLE users ADD COLUMN IF NOT EXISTS live_until TIMESTAMPTZ NOT NULL DEFAULT 'infinity';
