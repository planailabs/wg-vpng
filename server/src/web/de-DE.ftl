# wg-vpng UI strings (Deutsch).

## ── Allgemeine Aktionen ─────────────────────────────────────────
common-loading = Wird geladen…
action-generate = Erstellen
action-copy = Kopieren
action-show-config = Konfiguration anzeigen
action-regenerate = Neu erzeugen
action-delete = Löschen
action-config = Konfiguration
action-set = Setzen
action-add-device = Gerät hinzufügen
action-sign-out = Abmelden
action-revoke-access = Zugriff entziehen
action-restore-access = Zugriff wiederherstellen
action-ban = Sperren
action-unban = Entsperren
action-delete-user = Benutzer löschen

## ── Marke / Navigation ──────────────────────────────────────────
brand-title = WireGuard VPN
brand-subtitle = Generator
nav-devices = Meine Geräte
nav-interface = Schnittstelle
nav-users = Benutzer

## ── Meine Geräte ────────────────────────────────────────────────
devices-title = Meine Geräte
devices-subtitle = Erstelle eine WireGuard-Konfiguration für den Zugriff auf das Firmennetzwerk. Beim Neu-Erzeugen wird der Schlüssel ersetzt und die alte Konfiguration ungültig.
devices-new-name-label = Name des neuen Geräts
devices-new-name-placeholder = z. B. Laptop
devices-quota = { $used } von { $limit } Geräten verwendet.
devices-limit-reached = Gerätelimit erreicht.
devices-empty = Noch keine Geräte.
devices-config-heading = Konfiguration
device-unnamed = (ohne Namen)

## ── Benutzer (Admin) ────────────────────────────────────────────
users-title = Benutzer
users-subtitle = Verwalte die Geräte und den Zugriff jedes Benutzers. Ein Zugriffsentzug entfernt die Geräte aus dem VPN; eine Sperre blockiert zusätzlich die Anmeldung; Löschen entfernt den Benutzer samt aller Geräte.
users-empty = Noch keine Benutzer.
badge-admin = Admin
badge-banned = gesperrt
badge-revoked = entzogen
users-device-count = { $count } / { $limit } Geräte
users-limit-label = Limit
users-limit-placeholder = Std.
users-new-device-placeholder = Name des neuen Geräts
users-new-device-subnet-placeholder = Subnetz (optional, z. B. fd00:2::/64)
users-no-devices = Keine Geräte.
users-loading-devices = Geräte werden geladen…

## ── Schnittstelle ───────────────────────────────────────────────
interface-title = Schnittstelle
interface-subtitle = Die serverseitige WireGuard-Schnittstelle, mit der sich Clients verbinden.
iface-name = Name
iface-address-v4 = IPv4-Adresse
iface-address-v6 = IPv6-Adresse
iface-listen-port = Listen-Port
iface-endpoint = Endpunkt
iface-public-key = Öffentlicher Schlüssel
iface-routed-v4 = Geroutetes IPv4
iface-routed-v6 = Geroutetes IPv6
iface-dns = DNS
