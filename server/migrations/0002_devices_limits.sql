-- Per-user device limits and access control.
-- A "device" is a wg_peers row owned by a user (each has its own keypair/config).

ALTER TABLE users ADD COLUMN IF NOT EXISTS device_limit  INTEGER;               -- null = global default
ALTER TABLE users ADD COLUMN IF NOT EXISTS banned         BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE users ADD COLUMN IF NOT EXISTS access_revoked BOOLEAN NOT NULL DEFAULT false;
