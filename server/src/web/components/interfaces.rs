//! Admin interfaces page: full CRUD. Each interface has its own backend
//! (self-managed / NetworkManager / MikroTik), a per-user device limit, and a
//! pattern-based ACL. Backend bring-up failures are surfaced per interface.

use dioxus::prelude::*;
use dioxus_i18n::t;
use plan_ai_design::{Alert, AlertVariant, Button, ButtonVariant, Card};
use uuid::Uuid;

use crate::web::components::ui::PageHeader;
use crate::web::dto::InterfaceAdminView;
use crate::web::server_fns::{
    admin_create_interface, admin_delete_interface, admin_list_interfaces, admin_update_interface,
};

#[component]
pub fn Interfaces() -> Element {
    let mut refresh = use_signal(|| 0u32);
    let ifaces = use_server_future(move || {
        let _ = refresh();
        async move { admin_list_interfaces().await }
    })?;
    let mut error = use_signal(|| Option::<String>::None);

    rsx! {
        PageHeader { eyebrow: t!("nav-interface"), title: t!("interfaces-title"), subtitle: t!("interfaces-subtitle") }

        if let Some(e) = error() {
            Alert { variant: AlertVariant::Danger, class: "mb-4", "{e}" }
        }

        CreateInterface { on_change: move |_| { refresh += 1; }, on_error: move |e: String| error.set(Some(e)) }

        match &*ifaces.read() {
            Some(Ok(list)) if list.is_empty() => rsx! { p { class: "text-fg-muted text-sm mt-6", {t!("interfaces-empty")} } },
            Some(Ok(list)) => rsx! {
                div { class: "flex flex-col gap-4 mt-6",
                    for iface in list.clone() {
                        InterfaceCard {
                            key: "{iface.id}",
                            iface: iface.clone(),
                            on_change: move |_| { refresh += 1; },
                            on_error: move |e: String| error.set(Some(e)),
                        }
                    }
                }
            },
            Some(Err(e)) => rsx! { Alert { variant: AlertVariant::Danger, "{e}" } },
            None => rsx! { p { class: "text-fg-muted", {t!("common-loading")} } },
        }
    }
}

/// Backend selector + MikroTik fields, shared by create + edit forms.
#[component]
fn BackendFields(
    kind: Signal<String>,
    mk_url: Signal<String>,
    mk_user: Signal<String>,
    mk_pass: Signal<String>,
    mk_insecure: Signal<bool>,
    pass_placeholder: String,
    /// Lock the backend *type* (immutable after creation). MikroTik credential
    /// fields stay editable.
    #[props(default = false)]
    lock_kind: bool,
) -> Element {
    rsx! {
        div { class: "flex flex-col gap-2",
            label { class: "label text-sm text-fg-muted", {t!("if-field-backend")} }
            select {
                class: "input",
                value: "{kind}",
                disabled: lock_kind,
                onchange: move |e| kind.set(e.value()),
                option { value: "self-managed", {t!("if-backend-self-managed")} }
                option { value: "network-manager", {t!("if-backend-network-manager")} }
                option { value: "mikrotik", {t!("if-backend-mikrotik")} }
            }
            if kind() == "mikrotik" {
                input { class: "input text-sm", placeholder: t!("if-field-mikrotik-url"), value: "{mk_url}", oninput: move |e| mk_url.set(e.value()) }
                input { class: "input text-sm", placeholder: t!("if-field-mikrotik-username"), value: "{mk_user}", oninput: move |e| mk_user.set(e.value()) }
                input { class: "input text-sm", r#type: "password", placeholder: "{pass_placeholder}", value: "{mk_pass}", oninput: move |e| mk_pass.set(e.value()) }
                label { class: "flex items-center gap-2 text-sm text-fg-muted",
                    input { r#type: "checkbox", checked: mk_insecure(), onchange: move |e| mk_insecure.set(e.value() == "true") }
                    {t!("if-field-mikrotik-insecure")}
                }
            }
        }
    }
}

#[component]
fn CreateInterface(on_change: EventHandler<()>, on_error: EventHandler<String>) -> Element {
    let mut name = use_signal(|| "wg0".to_string());
    let mut display_name = use_signal(String::new);
    let mut listen_port = use_signal(|| "51820".to_string());
    let mut address = use_signal(|| "10.8.0.1/24, fd00:8::1/64".to_string());
    let mut endpoint = use_signal(String::new);
    let mut dns = use_signal(String::new);
    let mut allowed_ips = use_signal(|| "10.8.0.0/24, fd00:8::/64".to_string());
    let mut keepalive = use_signal(|| "25".to_string());
    let mut device_limit = use_signal(String::new);
    let mut patterns = use_signal(|| "*".to_string());
    let kind = use_signal(|| "self-managed".to_string());
    let mk_url = use_signal(String::new);
    let mk_user = use_signal(String::new);
    let mk_pass = use_signal(String::new);
    let mk_insecure = use_signal(|| false);

    let submit = move |_| async move {
        match admin_create_interface(
            name(),
            display_name(),
            listen_port().trim().parse().unwrap_or(51820),
            address(),
            endpoint(),
            dns(),
            allowed_ips(),
            keepalive().trim().parse().unwrap_or(25),
            device_limit().trim().parse::<i32>().ok(),
            patterns(),
            kind(),
            mk_url(),
            mk_user(),
            mk_pass(),
            mk_insecure(),
        )
        .await
        {
            Ok(()) => {
                endpoint.set(String::new());
                on_change.call(());
            }
            Err(e) => on_error.call(e.to_string()),
        }
    };

    rsx! {
        Card { class: "p-4",
            h3 { class: "text-sm font-semibold text-fg-strong mb-3", {t!("interfaces-create-heading")} }
            div { class: "grid grid-cols-1 sm:grid-cols-2 gap-3",
                Field { label: t!("if-field-name"), value: name, placeholder: "wg0".to_string() }
                Field { label: t!("if-field-display-name"), value: display_name, placeholder: t!("if-field-display-name-placeholder") }
                Field { label: t!("if-field-listen-port"), value: listen_port, placeholder: "51820".to_string() }
                Field { label: t!("if-field-address"), value: address, placeholder: "10.8.0.1/24, fd00::1/64".to_string() }
                Field { label: t!("if-field-endpoint"), value: endpoint, placeholder: "vpn.example.com:51820".to_string() }
                Field { label: t!("if-field-dns"), value: dns, placeholder: "10.8.0.1".to_string() }
                Field { label: t!("if-field-allowed-ips"), value: allowed_ips, placeholder: "0.0.0.0/0, ::/0".to_string() }
                Field { label: t!("if-field-keepalive"), value: keepalive, placeholder: "25".to_string() }
                Field { label: t!("if-field-device-limit"), value: device_limit, placeholder: "5".to_string() }
            }
            div { class: "mt-3",
                label { class: "label text-sm text-fg-muted", {t!("if-field-patterns")} }
                textarea { class: "input w-full text-sm font-mono", rows: "2", value: "{patterns}", oninput: move |e| patterns.set(e.value()) }
                p { class: "text-fg-faint text-xs mt-1", {t!("if-patterns-help")} }
            }
            div { class: "mt-3",
                BackendFields { kind, mk_url, mk_user, mk_pass, mk_insecure, pass_placeholder: t!("if-field-mikrotik-password") }
            }
            p { class: "text-fg-faint text-xs mt-3", {t!("if-immutable-note")} }
            div { class: "mt-3",
                Button { variant: ButtonVariant::Primary, onclick: submit, {t!("action-create")} }
            }
        }
    }
}

#[component]
fn Field(label: String, value: Signal<String>, placeholder: String) -> Element {
    rsx! {
        div {
            label { class: "label block text-sm text-fg-muted mb-1", "{label}" }
            input { class: "input w-full text-sm", placeholder: "{placeholder}", value: "{value}", oninput: move |e| value.set(e.value()) }
        }
    }
}

#[component]
fn InterfaceCard(iface: InterfaceAdminView, on_change: EventHandler<()>, on_error: EventHandler<String>) -> Element {
    let id = iface.id;
    let mut editing = use_signal(|| false);
    let mut display_name = use_signal(|| iface.display_name.clone());
    let mut endpoint = use_signal(|| iface.endpoint.clone());
    let mut dns = use_signal(|| iface.dns.clone().unwrap_or_default());
    let mut allowed_ips = use_signal(|| iface.allowed_ips.clone());
    let mut keepalive = use_signal(|| iface.keepalive.to_string());
    let mut device_limit = use_signal(|| iface.device_limit.map(|l| l.to_string()).unwrap_or_default());
    let mut patterns = use_signal(|| iface.access_patterns.join("\n"));
    let kind = use_signal(|| iface.backend_kind.clone());
    let mk_url = use_signal(|| iface.mikrotik_url.clone().unwrap_or_default());
    let mk_user = use_signal(|| iface.mikrotik_username.clone().unwrap_or_default());
    let mk_pass = use_signal(String::new);
    let mk_insecure = use_signal(|| iface.mikrotik_insecure);

    let save = move |_| async move {
        match admin_update_interface(
            id,
            display_name(),
            endpoint(),
            dns(),
            allowed_ips(),
            keepalive().trim().parse().unwrap_or(25),
            device_limit().trim().parse::<i32>().ok(),
            patterns(),
            kind(),
            mk_url(),
            mk_user(),
            mk_pass(),
            mk_insecure(),
        )
        .await
        {
            Ok(()) => { editing.set(false); on_change.call(()); }
            Err(e) => on_error.call(e.to_string()),
        }
    };

    rsx! {
        Card { class: "p-4",
            div { class: "flex items-center gap-3 flex-wrap",
                div { class: "flex-1 min-w-0",
                    div { class: "flex items-center gap-2 flex-wrap",
                        span { class: "text-fg-strong font-medium",
                            { if iface.display_name.is_empty() { iface.name.clone() } else { iface.display_name.clone() } }
                        }
                        span { class: "text-fg-faint text-xs font-mono px-1.5 py-0.5 rounded bg-surface-2", "{iface.name}" }
                    }
                    div { class: "text-fg-muted text-xs", "{iface.backend_kind} · {iface.address} · {iface.endpoint}" }
                }
                Button { variant: ButtonVariant::Secondary, onclick: move |_| editing.toggle(),
                    { if editing() { t!("action-cancel") } else { t!("action-edit") } }
                }
                Button {
                    variant: ButtonVariant::Danger,
                    onclick: move |_| async move {
                        match admin_delete_interface(id).await {
                            Ok(()) => on_change.call(()),
                            Err(e) => on_error.call(e.to_string()),
                        }
                    },
                    {t!("action-delete")}
                }
            }

            if let Some(be) = iface.backend_error.clone() {
                Alert { variant: AlertVariant::Danger, class: "mt-3",
                    {t!("interfaces-backend-error")} " " "{be}"
                }
            }

            div { class: "text-fg-muted text-xs mt-2 font-mono break-all",
                {t!("iface-public-key")} ": {iface.public_key}"
            }
            if !iface.access_patterns.is_empty() {
                div { class: "text-fg-muted text-xs mt-1", {t!("if-field-patterns")} ": {iface.access_patterns.join(\", \")}" }
            }

            if editing() {
                div { class: "mt-4 border-t border-line-soft pt-3 flex flex-col gap-3",
                    div { class: "grid grid-cols-1 sm:grid-cols-2 gap-3",
                        Field { label: t!("if-field-display-name"), value: display_name, placeholder: t!("if-field-display-name-placeholder") }
                        Field { label: t!("if-field-endpoint"), value: endpoint, placeholder: String::new() }
                        Field { label: t!("if-field-dns"), value: dns, placeholder: String::new() }
                        Field { label: t!("if-field-allowed-ips"), value: allowed_ips, placeholder: String::new() }
                        Field { label: t!("if-field-keepalive"), value: keepalive, placeholder: String::new() }
                        Field { label: t!("if-field-device-limit"), value: device_limit, placeholder: String::new() }
                    }
                    div {
                        label { class: "label text-sm text-fg-muted", {t!("if-field-patterns")} }
                        textarea { class: "input w-full text-sm font-mono", rows: "2", value: "{patterns}", oninput: move |e| patterns.set(e.value()) }
                    }
                    BackendFields { kind, mk_url, mk_user, mk_pass, mk_insecure, pass_placeholder: t!("if-field-mikrotik-password-keep"), lock_kind: true }
                    p { class: "text-fg-faint text-xs", {t!("if-immutable-note-edit")} }
                    div {
                        Button { variant: ButtonVariant::Primary, onclick: save, {t!("action-save")} }
                    }
                }
            }
        }
    }
}
