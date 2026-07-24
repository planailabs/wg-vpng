-- Per-interface download filename base for generated device configs.
-- NULL / empty falls back to the interface `name`. The downloaded file is
-- "<download_filename>-<device name>.conf".
ALTER TABLE wg_interfaces ADD COLUMN download_filename text;
