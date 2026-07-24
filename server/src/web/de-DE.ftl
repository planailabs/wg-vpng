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
nav-groups = Gruppen
nav-users = Benutzer
nav-tutorial = Anleitung

## ── Anleitung ───────────────────────────────────────────────────
tutorial-title = VPN einrichten
tutorial-subtitle = Wähle dein Betriebssystem und folge den Schritten, um ein Gerät zu verbinden.
tutorial-platform-linux = Linux
tutorial-platform-windows = Windows
tutorial-platform-android = Android

# Gemeinsame Schritte
tut-create = Wähle auf der Seite "Meine Geräte" die gewünschte Schnittstelle, gib einen Namen für das Gerät ein (z. B. "laptop") und klicke auf "Erstellen".
tut-download = Deine Konfiguration erscheint. Klicke auf "Herunterladen", um die .conf-Datei zu speichern (benannt nach Schnittstelle und Gerät). Jetzt speichern — sie wird nicht erneut angezeigt.
# Linux
tut-linux-save = Speichere die Datei am besten im Home-Verzeichnis und öffne ein Terminal. Liegt die Datei woanders, wechsle mit `cd` dorthin (z. B. `cd Downloads`). Die Befehle unten verwenden den Namen deiner heruntergeladenen Datei (hier `wg0-laptop.conf`) — ersetze ihn durch deinen. Unten findest du zwei Methoden — wähle eine und folge ihr.
tut-nm = NetworkManager
tut-linux-nm-intro = Falls deine Distribution NetworkManager verwendet, importiere und aktiviere die Konfiguration so:
tut-linux-nm-reboot = Die Verbindung wird nach Neustarts automatisch wiederhergestellt.
tut-tools = WireGuard Tools
tut-linux-tools-intro = Verwaltet deine Distribution das Netzwerk nicht über NetworkManager, nutze wireguard-tools (installiere das Paket `wireguard-tools`), und führe dann aus:
tut-linux-tools-reboot = Nach einem Neustart musst du die Verbindung eventuell erneut mit `sudo wg-quick up wg0-laptop` herstellen.
# Windows
tut-win-install = Installiere nun die WireGuard-Software: Lade sie von wireguard.com herunter und wähle die Windows-Version.
tut-win-download-link = WireGuard für Windows herunterladen
tut-win-open-file = Öffne die heruntergeladene Datei und bestätige die Sicherheitsmeldung.
tut-win-import = WireGuard sollte sich nach der Installation automatisch öffnen. Falls nicht, öffne die App WireGuard über die Suche. Wähle im WireGuard-Fenster "Tunnel aus Datei importieren".
tut-win-select = Wähle die Datei aus und bestätige mit "Öffnen".
tut-win-edit = Bearbeite anschließend die Verbindung.
tut-win-killswitch = Deaktiviere die Option "Verkehr außerhalb des Tunnels blockieren" und bestätige.
tut-win-activate = Aktiviere danach den Tunnel.
tut-win-done = Der Tunnel ist jetzt aktiv und wird nach einem Neustart automatisch neu gestartet.
# Android
tut-android-download = Deine Konfiguration erscheint. Lade die .conf-Datei herunter — oder scanne am Handy einfach den QR-Code direkt aus der WireGuard-App (siehe unten).
tut-android-install = Installiere nun die WireGuard-App aus dem Play Store.
tut-android-download-link = WireGuard bei Google Play holen
tut-android-open = Öffne die App.
tut-android-plus = Tippe unten rechts auf das Plus-Zeichen.
tut-android-import = Wähle "Aus Datei oder Archiv importieren".
tut-android-pick = Wähle Downloads und dann die heruntergeladene .conf-Datei.
tut-android-activate = Aktiviere den Tunnel.

## ── Gruppen (Admin) ─────────────────────────────────────────────
groups-title = Gruppen
groups-subtitle = Wiederverwendbare Zugriffsregeln. Ein Benutzer gehört zu einer Gruppe, wenn seine E-Mail einem Muster entspricht oder eine seiner OIDC-Claim-Gruppen aufgeführt ist. Schnittstellen gewähren Zugriff durch Zuweisen von Gruppen.
groups-empty = Noch keine Gruppen.
groups-create-heading = Neue Gruppe
group-field-name = Name
group-field-name-placeholder = z. B. engineering
group-field-patterns = E-Mail-Muster
group-field-claims = OIDC-Claim-Werte
group-claims-help = Werte, die mit der OIDC-Gruppen-Claim eines Benutzers abgeglichen werden (Claim-Pfad pro Anbieter). Beim Login erfasst.
if-field-groups = Gruppen (Zugriff)
if-no-groups = Noch keine Gruppen — erstelle eine auf der Gruppen-Seite.
if-no-groups-assigned = keine
action-resync = Neu synchronisieren
action-rename = Umbenennen

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
badge-deactivated = inaktiv
users-device-count = { $count } / { $limit } Geräte
users-limit-label = Limit
users-limit-placeholder = Std.
users-new-device-placeholder = Name des neuen Geräts
users-new-device-subnet-placeholder = Subnetz (optional: /64 oder fd00:2::/64)
users-no-devices = Keine Geräte.
users-loading-devices = Geräte werden geladen…

## ── Geräte / Schnittstellen (mehrere) ───────────────────────────
devices-no-interfaces = Für dich sind noch keine Schnittstellen verfügbar.
device-unconfigured-note = Nicht konfiguriert — Schlüssel generieren zum Aktivieren.
device-stale-note = Schnittstellen-Einstellungen geändert — neu generieren, um diese Konfiguration zu aktualisieren.
badge-unconfigured = nicht konfiguriert
badge-stale = veraltet
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
if-field-download-filename = Download-Dateiname
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
