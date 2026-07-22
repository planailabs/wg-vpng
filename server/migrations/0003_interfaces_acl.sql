-- Multiple interfaces, fully admin-managed: per-interface backend, device
-- limit, and pattern-based ACL. Interface settings no longer come from config.

ALTER TABLE wg_interfaces ADD COLUMN IF NOT EXISTS device_limit    INTEGER;                      -- NULL = unlimited
ALTER TABLE wg_interfaces ADD COLUMN IF NOT EXISTS access_patterns TEXT[] NOT NULL DEFAULT '{}'; -- globs / emails; * = everyone
-- The backend that applies THIS interface (self-managed / network-manager /
-- mikrotik + params), stored as the serialized BackendConfig. Secrets inside
-- (e.g. the MikroTik password) are encrypted with the app key below.
-- The backend that applies THIS interface. Secrets inside (the MikroTik
-- password) are encrypted with the app key from [secrets] encryption_key in
-- config.toml.
ALTER TABLE wg_interfaces ADD COLUMN IF NOT EXISTS backend JSONB NOT NULL DEFAULT '"self-managed"'::jsonb;
-- Last backend reconcile error (NULL when the last sync succeeded); surfaced in
-- the admin UI so interface bring-up failures are visible.
ALTER TABLE wg_interfaces ADD COLUMN IF NOT EXISTS last_error TEXT;
