-- Track the WireGuard config "version" of an interface: bumped whenever a value
-- that affects the rendered client config changes (endpoint, dns, allowed_ips,
-- keepalive). Each peer records the version it was last (re)generated against;
-- a peer whose version trails its interface has a stale downloaded config.
ALTER TABLE wg_interfaces ADD COLUMN config_version integer NOT NULL DEFAULT 1;
ALTER TABLE wg_peers ADD COLUMN config_version integer NOT NULL DEFAULT 1;
