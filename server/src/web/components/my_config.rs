//! "My devices" page: for each interface the user may access, their devices on
//! it (generate / regenerate / delete), bounded by the interface's per-user
//! device limit. The config (QR + text) is shown once, on generate/regenerate.

use dioxus::prelude::*;
use dioxus_i18n::t;
use plan_ai_design::{Alert, AlertVariant, Button, ButtonVariant, Card};
use uuid::Uuid;

use crate::web::components::ui::{ConfigPanel, PageHeader};
use crate::web::dto::{InterfaceAccessView, PeerView};
use crate::web::server_fns::{create_my_device, delete_peer, list_my_interfaces, my_devices, regenerate_peer};

#[component]
pub fn MyConfig() -> Element {
    let mut refresh = use_signal(|| 0u32);
    let ifaces = use_server_future(move || {
        let _ = refresh();
        async move { list_my_interfaces().await }
    })?;
    let devices = use_server_future(move || {
        let _ = refresh();
        async move { my_devices().await }
    })?;
    let mut error = use_signal(|| Option::<String>::None);
    // (config, qr_svg) of the most recently generated device — shown once.
    let mut shown = use_signal(|| Option::<(String, String)>::None);

    let iface_list = match &*ifaces.read() {
        Some(Ok(l)) => l.clone(),
        Some(Err(e)) => return rsx! { Alert { variant: AlertVariant::Danger, "{e}" } },
        None => return rsx! { p { class: "text-fg-muted", {t!("common-loading")} } },
    };
    let all_devices = match &*devices.read() {
        Some(Ok(l)) => l.clone(),
        _ => vec![],
    };

    rsx! {
        PageHeader { eyebrow: t!("nav-devices"), title: t!("devices-title"), subtitle: t!("devices-subtitle") }

        if let Some(e) = error() {
            Alert { variant: AlertVariant::Danger, class: "mb-4", "{e}" }
        }

        if iface_list.is_empty() {
            p { class: "text-fg-muted text-sm", {t!("devices-no-interfaces")} }
        }

        div { class: "flex flex-col gap-6",
            for iface in iface_list.clone() {
                InterfaceSection {
                    key: "{iface.id}",
                    iface: iface.clone(),
                    devices: all_devices.iter().filter(|d| d.interface_id == iface.id).cloned().collect::<Vec<_>>(),
                    on_change: move |_| refresh.with_mut(|r| *r += 1),
                    on_show: move |c: (String, String)| shown.set(Some(c)),
                    on_error: move |e: String| error.set(Some(e)),
                }
            }
        }

        if let Some((config, qr_svg)) = shown() {
            ConfigPanel { config, qr_svg }
        }
    }
}

#[component]
fn InterfaceSection(
    iface: InterfaceAccessView,
    devices: Vec<PeerView>,
    on_change: EventHandler<()>,
    on_show: EventHandler<(String, String)>,
    on_error: EventHandler<String>,
) -> Element {
    let iid = iface.id;
    let mut new_name = use_signal(String::new);
    let used = iface.used;
    let at_limit = iface.limit.map(|l| used >= l as i64).unwrap_or(false);
    let quota = match iface.limit {
        Some(l) => t!("devices-quota", used: used, limit: l),
        None => t!("devices-quota-unlimited", used: used),
    };

    rsx! {
        Card { class: "p-0 overflow-hidden",
            div { class: "px-4 py-3 border-b border-line-soft bg-surface-2 flex items-center gap-3 flex-wrap",
                div { class: "flex-1 min-w-0",
                    div { class: "text-fg-strong font-medium", "{iface.name}" }
                    div { class: "text-fg-muted text-xs", "{iface.endpoint} · {quota}" }
                }
                input {
                    class: "input w-40 text-sm",
                    placeholder: t!("devices-new-name-placeholder"),
                    value: "{new_name}",
                    oninput: move |e| new_name.set(e.value()),
                }
                Button {
                    variant: ButtonVariant::Primary,
                    disabled: at_limit,
                    onclick: move |_| {
                        let name = new_name();
                        async move {
                            match create_my_device(iid, name).await {
                                Ok(v) => {
                                    new_name.set(String::new());
                                    on_show.call((v.config, v.qr_svg));
                                    on_change.call(());
                                }
                                Err(e) => on_error.call(e.to_string()),
                            }
                        }
                    },
                    {t!("action-generate")}
                }
            }

            div { class: "p-4",
                if devices.is_empty() {
                    p { class: "text-fg-muted text-xs", {t!("devices-empty")} }
                }
                div { class: "flex flex-col gap-2",
                    for peer in devices.clone() {
                        DeviceRow {
                            key: "{peer.id}",
                            id: peer.id,
                            name: peer.name.clone(),
                            address: peer.address.clone(),
                            configured: peer.configured,
                            on_change: move |_| on_change.call(()),
                            on_show: move |c: (String, String)| on_show.call(c),
                            on_error: move |e: String| on_error.call(e),
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn DeviceRow(
    id: Uuid,
    name: String,
    address: String,
    configured: bool,
    on_change: EventHandler<()>,
    on_show: EventHandler<(String, String)>,
    on_error: EventHandler<String>,
) -> Element {
    let display_name = if name.is_empty() { t!("device-unnamed") } else { name };
    rsx! {
        div { class: "flex items-center gap-3 border-t border-line-soft pt-2 first:border-0 first:pt-0",
            div { class: "flex-1 min-w-0",
                div { class: "text-fg-strong text-sm", "{display_name}" }
                if configured {
                    div { class: "text-fg-muted text-xs font-mono", "{address}" }
                } else {
                    div { class: "text-warn text-xs", {t!("device-unconfigured-note")} }
                }
            }
            Button {
                // Both "generate" (unconfigured) and "regenerate" (configured)
                // produce a fresh key + config, shown once.
                variant: if configured { ButtonVariant::Secondary } else { ButtonVariant::Primary },
                onclick: move |_| async move {
                    match regenerate_peer(id).await {
                        Ok(v) => { on_show.call((v.config, v.qr_svg)); on_change.call(()); }
                        Err(e) => on_error.call(e.to_string()),
                    }
                },
                { if configured { t!("action-regenerate") } else { t!("action-generate") } }
            }
            Button {
                variant: ButtonVariant::Danger,
                onclick: move |_| async move {
                    match delete_peer(id).await {
                        Ok(()) => on_change.call(()),
                        Err(e) => on_error.call(e.to_string()),
                    }
                },
                {t!("action-delete")}
            }
        }
    }
}
