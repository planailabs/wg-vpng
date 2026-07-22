-- Access patterns were a Postgres TEXT[]; store them as a jsonb array instead so
-- the app can bind/decode them as a plain JSON list. to_jsonb preserves order.
ALTER TABLE wg_interfaces
    ALTER COLUMN access_patterns DROP DEFAULT,
    ALTER COLUMN access_patterns TYPE jsonb USING to_jsonb(access_patterns),
    ALTER COLUMN access_patterns SET DEFAULT '[]'::jsonb;
