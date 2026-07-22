-- "Unconfigured" devices: an admin can create a device without generating a
-- key, so the user generates it themselves (public_key stays empty until then).
-- Empty public keys must not collide, so replace the plain UNIQUE with a
-- partial unique index that ignores empty keys.
ALTER TABLE wg_peers DROP CONSTRAINT IF EXISTS wg_peers_interface_id_public_key_key;
CREATE UNIQUE INDEX IF NOT EXISTS wg_peers_iface_pubkey_uidx
    ON wg_peers (interface_id, public_key) WHERE public_key <> '';
