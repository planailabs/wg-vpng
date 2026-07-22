//! Admin view of the managed WireGuard interface.

use dioxus::prelude::*;
use dioxus_i18n::t;
use plan_ai_design::{Alert, AlertVariant, Card};

use crate::web::server_fns::get_interface;

#[component]
pub fn Interfaces() -> Element {
    let iface = use_server_future(get_interface)?;

    rsx! {
        div { class: "mb-8",
            h2 { class: "text-2xl font-semibold text-fg-strong tracking-tight", {t!("interface-title")} }
            p { class: "text-fg-muted text-sm mt-1", {t!("interface-subtitle")} }
        }
        match &*iface.read() {
            Some(Ok(i)) => rsx! {
                Card { class: "p-6 max-w-xl mx-auto",
                    Row { label: t!("iface-name"), value: i.name.clone() }
                    Row { label: t!("iface-address-v4"), value: family(&i.address, false) }
                    Row { label: t!("iface-address-v6"), value: family(&i.address, true) }
                    Row { label: t!("iface-listen-port"), value: i.listen_port.to_string() }
                    Row { label: t!("iface-endpoint"), value: i.endpoint.clone() }
                    Row { label: t!("iface-public-key"), value: i.public_key.clone() }
                    Row { label: t!("iface-routed-v4"), value: family(&i.allowed_ips, false) }
                    Row { label: t!("iface-routed-v6"), value: family(&i.allowed_ips, true) }
                    Row { label: t!("iface-dns"), value: i.dns.clone().unwrap_or_else(|| "—".into()) }
                }
            },
            Some(Err(e)) => rsx! { Alert { variant: AlertVariant::Danger, "{e}" } },
            None => rsx! { p { class: "text-fg-muted", {t!("common-loading")} } },
        }
    }
}

/// Pick the IPv4 (`v6 = false`) or IPv6 (`v6 = true`) members of a comma-list
/// address spec; `—` when that family is absent.
fn family(spec: &str, v6: bool) -> String {
    let parts: Vec<&str> = spec
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter(|s| s.contains(':') == v6)
        .collect();
    if parts.is_empty() { "—".to_string() } else { parts.join(", ") }
}

#[component]
fn Row(label: String, value: String) -> Element {
    rsx! {
        div { class: "flex py-1.5 border-b border-line-soft last:border-0",
            div { class: "w-40 text-fg-muted text-sm", "{label}" }
            div { class: "flex-1 text-fg text-sm break-all font-mono", "{value}" }
        }
    }
}
