# wg-vpng UI strings (English). Concatenated with plan-ai-design's shared
# translations at runtime. German lives in de-DE.ftl.

## ── Common actions ──────────────────────────────────────────────
common-loading = Loading…
action-generate = Generate
action-copy = Copy
action-download = Download
devices-scan-hint = Scan the QR with the WireGuard app, or copy/download the config. Save it now — it can't be shown again.
action-show-config = Show config
action-regenerate = Regenerate
action-delete = Delete
action-config = Config
action-set = Set
action-add-device = Add device
action-add-pattern = Add pattern
action-remove = Remove
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
nav-groups = Groups
nav-users = Users
nav-tutorial = Setup guide

## ── Setup guide ─────────────────────────────────────────────────
tutorial-title = Set up your VPN
tutorial-subtitle = Pick your operating system and follow the steps to connect a device.
tutorial-platform-linux = Linux
tutorial-platform-windows = Windows
tutorial-platform-android = Android

# Shared steps
tut-create = On the "My devices" page, pick the interface you want, type a name for this device (e.g. "laptop") and click Generate.
tut-download = Your configuration appears. Click Download to save the .conf file (named after the interface and your device). Save it now — it can't be shown again.
# Linux
tut-linux-save = Save the file in your home directory, then open a terminal. If it was saved elsewhere, change into that directory with `cd` (e.g. `cd Downloads`). The commands below use the name of the file you downloaded (shown here as `wg0-laptop.conf`) — substitute yours. Below are two methods — pick one and follow it.
tut-nm = NetworkManager
tut-linux-nm-intro = If your distribution uses NetworkManager, import and activate the config with:
tut-linux-nm-reboot = The connection is re-established automatically after a reboot.
tut-tools = WireGuard tools
tut-linux-tools-intro = If your network is not managed by NetworkManager, use wireguard-tools (install the `wireguard-tools` package for your distribution), then run:
tut-linux-tools-reboot = After a reboot you may need to bring the connection up again with `sudo wg-quick up wg0-laptop`.
# Windows
tut-win-install = Now install the WireGuard software: download it from wireguard.com and choose the Windows version.
tut-win-download-link = Download WireGuard for Windows
tut-win-open-file = After downloading, open the file and accept the security prompt.
tut-win-import = WireGuard should open automatically after installation. If not, just open the WireGuard app from the search. In the WireGuard window, choose "Import tunnel(s) from file".
tut-win-select = Select the file and confirm with "Open".
tut-win-edit = After importing, edit the connection.
tut-win-killswitch = Disable the option "Block untunneled traffic (kill-switch)" and confirm.
tut-win-activate = Then activate the tunnel.
tut-win-done = The tunnel is now active. It reconnects automatically after a reboot.
# Android
tut-android-download = Your configuration appears. Download the .conf file — or, quicker on a phone, scan the QR code directly from the WireGuard app (see below).
tut-android-install = Now install the WireGuard app from the Play Store.
tut-android-download-link = Get WireGuard on Google Play
tut-android-open = Open the app.
tut-android-plus = Tap the plus button at the bottom right.
tut-android-import = Choose "Import from file or archive".
tut-android-pick = Pick Downloads, then the .conf file you downloaded.
tut-android-activate = Activate the tunnel.

## ── Groups (admin) ──────────────────────────────────────────────
groups-title = Groups
groups-subtitle = Reusable access rules. A user belongs to a group if their email matches a pattern, or one of their OIDC claim groups is listed. Interfaces grant access by assigning groups.
groups-empty = No groups yet.
groups-create-heading = New group
group-field-name = Name
group-field-name-placeholder = e.g. engineering
group-field-patterns = Email patterns
group-field-claims = OIDC claim values
group-claims-help = Values matched against a user's OIDC group claim (per-provider claim path). Captured at login.
if-field-groups = Groups (access)
if-no-groups = No groups yet — create one on the Groups page.
if-no-groups-assigned = none
action-resync = Re-sync
action-rename = Rename

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
badge-deactivated = inactive
users-device-count = { $count } / { $limit } devices
users-limit-label = Limit
users-limit-placeholder = def
users-new-device-placeholder = new device name
users-new-device-subnet-placeholder = subnet (optional: /64 or fd00:2::/64)
users-no-devices = No devices.
users-loading-devices = Loading devices…

## ── Devices / interfaces (multi-interface) ──────────────────────
devices-no-interfaces = No interfaces are available to you yet.
device-unconfigured-note = Not configured — generate a key to activate.
device-stale-note = Interface settings changed — regenerate to update this config.
badge-unconfigured = unconfigured
badge-stale = outdated
admin-generate-key = Generate key now
devices-quota-unlimited = { $used } devices
users-device-count-simple = { $count } devices
action-create = Create
action-save = Save
action-edit = Edit
action-cancel = Cancel

## ── Interfaces admin ────────────────────────────────────────────
interfaces-title = Interfaces
interfaces-subtitle = Create and manage WireGuard interfaces. Each has its own backend, per-user device limit, and access patterns.
interfaces-create-heading = New interface
interfaces-empty = No interfaces yet.
interfaces-backend-error = Backend error:
if-field-name = Interface id (e.g. wg0)
if-field-display-name = Display name
if-field-download-filename = Download filename
if-field-display-name-placeholder = e.g. Office VPN
if-field-listen-port = Listen port
if-field-address = Server address(es)
if-field-endpoint = Endpoint
if-field-dns = DNS
if-field-allowed-ips = Routed networks
if-field-keepalive = Keepalive
if-field-device-limit = Device limit (per user)
if-field-patterns = Access patterns
if-patterns-help = One per line or space. * = everyone; literal emails allowed. Empty = nobody.
if-field-backend = Backend
if-backend-self-managed = Self-managed (wg + ip)
if-backend-network-manager = NetworkManager
if-backend-systemd-networkd = systemd-networkd
if-backend-mikrotik = MikroTik (RouterOS)
if-backend-node = Node (remote agent)
if-field-node-url = Node URL (http://host:8787)
if-field-node-key = Node API key
if-field-node-key-keep = Node API key (leave blank to keep)
if-field-mikrotik-url = RouterOS URL (https://…)
if-field-mikrotik-username = RouterOS username
if-field-mikrotik-password = RouterOS password
if-field-mikrotik-password-keep = RouterOS password (leave blank to keep)
if-field-mikrotik-insecure = Accept self-signed certificate
if-immutable-note = Server addresses and the backend type can't be changed after creation.
if-immutable-note-edit = Server addresses and the backend type can't be changed after creation. MikroTik credentials can still be updated here.

## ── Interface (legacy overview keys) ────────────────────────────
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
