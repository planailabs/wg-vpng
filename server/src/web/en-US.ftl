# wg-vpng UI strings (English). Concatenated with plan-ai-design's shared
# translations at runtime. German lives in de-DE.ftl.

## ── Common actions ──────────────────────────────────────────────
common-loading = Loading…
action-generate = Generate
action-copy = Copy
action-show-config = Show config
action-regenerate = Regenerate
action-delete = Delete
action-config = Config
action-set = Set
action-add-device = Add device
action-sign-out = Sign out
action-revoke-access = Revoke access
action-restore-access = Restore access
action-ban = Ban
action-unban = Unban
action-delete-user = Delete user

## ── Brand / navigation ──────────────────────────────────────────
brand-title = WireGuard VPN
brand-subtitle = Generator
nav-devices = My devices
nav-interface = Interface
nav-users = Users

## ── My devices ──────────────────────────────────────────────────
devices-title = My devices
devices-subtitle = Generate a WireGuard configuration to reach the company network. Regenerating replaces the key and invalidates the old config.
devices-new-name-label = New device name
devices-new-name-placeholder = e.g. laptop
devices-quota = { $used } of { $limit } devices used.
devices-limit-reached = Device limit reached.
devices-empty = No devices yet.
devices-config-heading = Configuration
device-unnamed = (unnamed)

## ── Users (admin) ───────────────────────────────────────────────
users-title = Users
users-subtitle = Manage each user's devices and access. Revoking drops a user's devices from the VPN; banning also blocks login; deleting removes the user and all their devices.
users-empty = No users yet.
badge-admin = admin
badge-banned = banned
badge-revoked = revoked
users-device-count = { $count } / { $limit } devices
users-limit-label = Limit
users-limit-placeholder = def
users-new-device-placeholder = new device name
users-new-device-subnet-placeholder = subnet (optional, e.g. fd00:2::/64)
users-no-devices = No devices.
users-loading-devices = Loading devices…

## ── Interface ───────────────────────────────────────────────────
interface-title = Interface
interface-subtitle = The server-side WireGuard interface clients connect to.
iface-name = Name
iface-address-v4 = IPv4 address
iface-address-v6 = IPv6 address
iface-listen-port = Listen port
iface-endpoint = Endpoint
iface-public-key = Public key
iface-routed-v4 = Routed IPv4
iface-routed-v6 = Routed IPv6
iface-dns = DNS
