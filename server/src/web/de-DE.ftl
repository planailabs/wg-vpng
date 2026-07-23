# wg-vpng UI strings (Deutsch).

## ── Allgemeine Aktionen ─────────────────────────────────────────
common-loading = Wird geladen…
action-generate = Erstellen
action-copy = Kopieren
action-download = Herunterladen
devices-scan-hint = Scanne den QR-Code mit der WireGuard-App oder kopiere/lade die Konfiguration herunter. Jetzt speichern — sie wird nicht erneut angezeigt.
action-show-config = Konfiguration anzeigen
action-regenerate = Neu erzeugen
action-delete = Löschen
action-config = Konfiguration
action-set = Setzen
action-add-device = Gerät hinzufügen
action-add-pattern = Muster hinzufügen
action-remove = Entfernen
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

## ── Geräte / Schnittstellen (mehrere) ───────────────────────────
devices-no-interfaces = Für dich sind noch keine Schnittstellen verfügbar.
device-unconfigured-note = Nicht konfiguriert — Schlüssel generieren zum Aktivieren.
badge-unconfigured = nicht konfiguriert
admin-generate-key = Schlüssel jetzt generieren
devices-quota-unlimited = { $used } Geräte
users-device-count-simple = { $count } Geräte
action-create = Erstellen
action-save = Speichern
action-edit = Bearbeiten
action-cancel = Abbrechen

## ── Schnittstellen-Verwaltung ───────────────────────────────────
interfaces-title = Schnittstellen
interfaces-subtitle = WireGuard-Schnittstellen erstellen und verwalten. Jede hat ihr eigenes Backend, ein Gerätelimit pro Benutzer und Zugriffsmuster.
interfaces-create-heading = Neue Schnittstelle
interfaces-empty = Noch keine Schnittstellen.
interfaces-backend-error = Backend-Fehler:
if-field-name = Schnittstellen-ID (z. B. wg0)
if-field-display-name = Anzeigename
if-field-display-name-placeholder = z. B. Büro-VPN
if-field-listen-port = Listen-Port
if-field-address = Server-Adresse(n)
if-field-endpoint = Endpunkt
if-field-dns = DNS
if-field-allowed-ips = Geroutete Netzwerke
if-field-keepalive = Keepalive
if-field-device-limit = Gerätelimit (pro Benutzer)
if-field-patterns = Zugriffsmuster
if-patterns-help = Eins pro Zeile oder Leerzeichen. * = alle; literale E-Mails erlaubt. Leer = niemand.
if-field-backend = Backend
if-backend-self-managed = Selbstverwaltet (wg + ip)
if-backend-network-manager = NetworkManager
if-backend-systemd-networkd = systemd-networkd
if-backend-mikrotik = MikroTik (RouterOS)
if-backend-node = Node (Remote-Agent)
if-field-node-url = Node-URL (http://host:8787)
if-field-node-key = Node-API-Schlüssel
if-field-node-key-keep = Node-API-Schlüssel (leer lassen zum Beibehalten)
if-field-mikrotik-url = RouterOS-URL (https://…)
if-field-mikrotik-username = RouterOS-Benutzername
if-field-mikrotik-password = RouterOS-Passwort
if-field-mikrotik-password-keep = RouterOS-Passwort (leer lassen zum Beibehalten)
if-field-mikrotik-insecure = Selbstsigniertes Zertifikat akzeptieren
if-immutable-note = Server-Adressen und der Backend-Typ können nach dem Erstellen nicht geändert werden.
if-immutable-note-edit = Server-Adressen und der Backend-Typ können nach dem Erstellen nicht geändert werden. MikroTik-Zugangsdaten können hier weiterhin aktualisiert werden.

## ── Schnittstelle (alte Übersichts-Keys) ────────────────────────
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
