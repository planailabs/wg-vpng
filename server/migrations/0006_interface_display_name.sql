-- `name` is the WireGuard interface id (e.g. wg0) and is immutable + unique.
-- `display_name` is a human-friendly label admins can edit freely. Empty means
-- "fall back to name" in the UI, so existing rows need no backfill.
ALTER TABLE wg_interfaces ADD COLUMN IF NOT EXISTS display_name TEXT NOT NULL DEFAULT '';
