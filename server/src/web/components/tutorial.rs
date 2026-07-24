//! Setup guide: per-platform, step-by-step instructions for importing a
//! generated config into the WireGuard client. Adapted from the legacy Vue
//! tutorial. Content is bilingual (en/de) inline — the images (served from
//! `public/tutorial/`) are language-agnostic. Platform is chosen locally; the
//! language follows the app-wide i18n selection.

use dioxus::prelude::*;
use dioxus_i18n::{prelude::i18n, t};
use plan_ai_design::Card;

use crate::web::components::ui::PageHeader;

#[derive(Clone, Copy, PartialEq)]
enum Platform {
    Linux,
    Windows,
    Android,
}

/// One rendered block of a tutorial. Text/Heading are bilingual; Code and Img
/// are the same regardless of language.
enum Block {
    Heading { en: &'static str, de: &'static str },
    Text { en: &'static str, de: &'static str },
    Code(&'static str),
    /// Image basename under `/tutorial/`.
    Img(&'static str),
}

/// Pick English or German for the current language (default English).
fn is_de() -> bool {
    i18n().language().to_string() == "de-DE"
}

#[component]
pub fn Tutorial() -> Element {
    let mut platform = use_signal(|| Platform::Windows);
    let de = is_de();
    let current = platform();

    rsx! {
        PageHeader { eyebrow: t!("nav-tutorial"), title: t!("tutorial-title"), subtitle: t!("tutorial-subtitle") }

        div { class: "flex gap-2 mb-6 flex-wrap",
            PlatformTab { label: t!("tutorial-platform-linux"), active: current == Platform::Linux,
                onclick: move |_| platform.set(Platform::Linux) }
            PlatformTab { label: t!("tutorial-platform-windows"), active: current == Platform::Windows,
                onclick: move |_| platform.set(Platform::Windows) }
            PlatformTab { label: t!("tutorial-platform-android"), active: current == Platform::Android,
                onclick: move |_| platform.set(Platform::Android) }
        }

        Card { class: "p-6",
            div { class: "flex flex-col gap-4 max-w-3xl",
                for (i, block) in blocks(current).into_iter().enumerate() {
                    match block {
                        Block::Heading { en, de: d } => rsx! {
                            h3 { key: "{i}", class: "text-lg font-semibold text-fg-strong mt-4", { if de { d } else { en } } }
                        },
                        Block::Text { en, de: d } => rsx! {
                            p { key: "{i}", class: "text-fg-muted text-sm leading-relaxed", { if de { d } else { en } } }
                        },
                        Block::Code(code) => rsx! {
                            pre { key: "{i}", class: "text-xs bg-surface-2 rounded-md p-3 overflow-x-auto whitespace-pre text-fg-strong", "{code}" }
                        },
                        Block::Img(name) => rsx! {
                            img {
                                key: "{i}",
                                class: "rounded-lg border border-line max-h-[420px] w-auto shadow-card",
                                src: "/tutorial/{name}",
                                loading: "lazy",
                                alt: "",
                            }
                        },
                    }
                }
            }
        }
    }
}

#[component]
fn PlatformTab(label: String, active: bool, onclick: EventHandler<MouseEvent>) -> Element {
    let cls = if active {
        "px-4 py-2 rounded-lg text-sm font-medium bg-brand text-fg-invert shadow-card"
    } else {
        "px-4 py-2 rounded-lg text-sm font-medium bg-surface-2 text-fg-muted hover:text-fg-strong transition-colors"
    };
    rsx! {
        button { class: "{cls}", onclick: move |e| onclick.call(e), "{label}" }
    }
}

fn blocks(platform: Platform) -> Vec<Block> {
    match platform {
        Platform::Linux => linux(),
        Platform::Windows => windows(),
        Platform::Android => android(),
    }
}

// The shared opening line: download a config first.
const INTRO_EN: &str =
    "First download a configuration: create a device under \"My devices\", or pick \"Regenerate\" on an existing one.";
const INTRO_DE: &str =
    "Lade zuerst eine Konfiguration herunter, indem du unter \"Meine Geräte\" ein Gerät erstellst oder bei einem bestehenden Gerät \"Neu erzeugen\" wählst.";

fn linux() -> Vec<Block> {
    vec![
        Block::Text { en: INTRO_EN, de: INTRO_DE },
        Block::Text {
            en: "Save the file in your home directory, then open a terminal. If the file was saved elsewhere, change into that directory with `cd` (e.g. `cd Downloads`). Below are two methods — pick one and follow it.",
            de: "Speichere die Datei am besten im Home-Verzeichnis und öffne ein Terminal. Liegt die Datei woanders, wechsle mit `cd` dorthin (z. B. `cd Downloads`). Unten findest du zwei Methoden — wähle eine und folge ihr.",
        },
        Block::Heading { en: "NetworkManager", de: "NetworkManager" },
        Block::Text {
            en: "If your distribution uses NetworkManager, import and activate the config with:",
            de: "Falls deine Distribution NetworkManager verwendet, importiere und aktiviere die Konfiguration so:",
        },
        Block::Code("nmcli conn import type wireguard file ip46.conf\nnmcli conn mod ip46 ipv6.route-metric 9000\n# start it\nnmcli c u ip46"),
        Block::Text {
            en: "The connection is re-established automatically after a reboot.",
            de: "Die Verbindung wird nach Neustarts automatisch wiederhergestellt.",
        },
        Block::Heading { en: "WireGuard tools", de: "WireGuard Tools" },
        Block::Text {
            en: "If your network is not managed by NetworkManager, use wireguard-tools (install the `wireguard-tools` package for your distribution), then run:",
            de: "Verwaltet deine Distribution das Netzwerk nicht über NetworkManager, nutze wireguard-tools (installiere das Paket `wireguard-tools`), und führe dann aus:",
        },
        Block::Code("sudo mkdir -p /etc/wireguard\nsudo cp ip46.conf /etc/wireguard/ip46.conf\nsudo wg-quick up ip46"),
        Block::Text {
            en: "After a reboot you may need to bring the connection up again with `sudo wg-quick up ip46`.",
            de: "Nach einem Neustart musst du die Verbindung eventuell erneut mit `sudo wg-quick up ip46` herstellen.",
        },
    ]
}

fn windows() -> Vec<Block> {
    vec![
        Block::Text { en: INTRO_EN, de: INTRO_DE },
        Block::Img("win-no-tunnels.png"),
        Block::Img("win-add-tunnel.png"),
        Block::Text {
            en: "After generating the tunnel you should see a download at the bottom of the screen.",
            de: "Nach dem Erzeugen des Tunnels sollte am unteren Bildschirmrand ein Download erscheinen.",
        },
        Block::Img("win-post-download.png"),
        Block::Text {
            en: "Now install the WireGuard software: go to https://wireguard.com/install/ and choose the Windows version.",
            de: "Installiere nun die WireGuard-Software: Gehe auf https://wireguard.com/install/ und wähle die Windows-Version.",
        },
        Block::Img("win-wireguard-com.png"),
        Block::Text {
            en: "After downloading, open the file and accept the security prompt.",
            de: "Öffne die heruntergeladene Datei und bestätige die Sicherheitsmeldung.",
        },
        Block::Img("win-wireguard-com-dl2.png"),
        Block::Img("win-wireguard-smartscreen.png"),
        Block::Text {
            en: "WireGuard should open automatically after installation. If not, just open the WireGuard app from the search. In the WireGuard window, choose \"Import tunnel(s) from file\".",
            de: "WireGuard sollte sich nach der Installation automatisch öffnen. Falls nicht, öffne die App WireGuard über die Suche. Wähle im WireGuard-Fenster \"Tunnel aus Datei importieren\".",
        },
        Block::Img("win-wireguard-open.png"),
        Block::Text {
            en: "Select the file and confirm with \"Open\".",
            de: "Wähle die Datei aus und bestätige mit \"Öffnen\".",
        },
        Block::Img("win-wireguard-file-select.png"),
        Block::Text {
            en: "After importing, edit the connection.",
            de: "Bearbeite anschließend die Verbindung.",
        },
        Block::Img("win-wireguard-edit.png"),
        Block::Text {
            en: "Disable the option \"Block untunneled traffic (kill-switch)\" and confirm.",
            de: "Deaktiviere die Option \"Verkehr außerhalb des Tunnels blockieren\" und bestätige.",
        },
        Block::Img("win-wireguard-edit2.png"),
        Block::Text {
            en: "Then activate the tunnel.",
            de: "Aktiviere danach den Tunnel.",
        },
        Block::Img("win-wireguard-activate.png"),
        Block::Img("win-wireguard-activate2.png"),
        Block::Text {
            en: "The tunnel is now active. It reconnects automatically after a reboot.",
            de: "Der Tunnel ist jetzt aktiv und wird nach einem Neustart automatisch neu gestartet.",
        },
    ]
}

fn android() -> Vec<Block> {
    vec![
        Block::Text { en: INTRO_EN, de: INTRO_DE },
        Block::Img("android-no-tunnels.png"),
        Block::Img("android-add-tunnel.png"),
        Block::Text {
            en: "After generating the tunnel you should see a message about a successful download.",
            de: "Nach dem Erzeugen des Tunnels sollte eine Meldung über einen erfolgreichen Download erscheinen.",
        },
        Block::Img("android-post-download.png"),
        Block::Text {
            en: "Now install the WireGuard app from the Play Store.",
            de: "Installiere nun die WireGuard-App aus dem Play Store.",
        },
        Block::Img("android-wireguard-playstore.png"),
        Block::Text {
            en: "Open the app.",
            de: "Öffne die App.",
        },
        Block::Img("android-wireguard-playstore-installed.png"),
        Block::Text {
            en: "Tap the plus button at the bottom right.",
            de: "Tippe unten rechts auf das Plus-Zeichen.",
        },
        Block::Img("android-wireguard-open.png"),
        Block::Text {
            en: "Choose \"Import from file or archive\".",
            de: "Wähle \"Aus Datei oder Archiv importieren\".",
        },
        Block::Img("android-wireguard-import.png"),
        Block::Text {
            en: "Pick Downloads, then the file ip46.conf.",
            de: "Wähle Downloads und dann die Datei ip46.conf.",
        },
        Block::Img("android-wireguard-import-1.png"),
        Block::Text {
            en: "Activate the tunnel.",
            de: "Aktiviere den Tunnel.",
        },
    ]
}
