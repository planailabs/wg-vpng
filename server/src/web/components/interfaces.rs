//! Admin view of the managed WireGuard interface.

use dioxus::prelude::*;
use plan_ai_design::{Alert, AlertVariant, Card};

use crate::web::server_fns::get_interface;

#[component]
pub fn Interfaces() -> Element {
    let iface = use_server_future(get_interface)?;

    rsx! {
        h2 { class: "text-xl font-semibold text-fg-strong mb-6", "Interface" }
        match &*iface.read() {
            Some(Ok(i)) => rsx! {
                Card { class: "p-4 max-w-xl",
                    Row { label: "Name", value: i.name.clone() }
                    Row { label: "Address", value: i.address.clone() }
                    Row { label: "Listen port", value: i.listen_port.to_string() }
                    Row { label: "Endpoint", value: i.endpoint.clone() }
                    Row { label: "Public key", value: i.public_key.clone() }
                    Row { label: "Routed networks", value: i.allowed_ips.clone() }
                    Row { label: "DNS", value: i.dns.clone().unwrap_or_else(|| "—".into()) }
                }
            },
            Some(Err(e)) => rsx! { Alert { variant: AlertVariant::Danger, "{e}" } },
            None => rsx! { p { class: "text-fg-muted", "Loading…" } },
        }
    }
}

#[component]
fn Row(label: &'static str, value: String) -> Element {
    rsx! {
        div { class: "flex py-1.5 border-b border-line-soft last:border-0",
            div { class: "w-40 text-fg-muted text-sm", "{label}" }
            div { class: "flex-1 text-fg text-sm break-all font-mono", "{value}" }
        }
    }
}
