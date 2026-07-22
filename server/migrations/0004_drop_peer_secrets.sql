-- The client's private key is never needed server-side (only its public key is,
-- to configure the peer), so it must not be stored. It is generated fresh and
-- shown once at creation/regeneration. The preshared key is kept — it's a
-- server-side secret needed to configure the peer.
ALTER TABLE wg_peers DROP COLUMN IF EXISTS private_key;
