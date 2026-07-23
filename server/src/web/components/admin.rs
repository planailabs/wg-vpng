//! Admin Users page: per-user device management (create on any interface,
//! regenerate, delete, show config) plus access control — revoke, ban, delete.
//! Interface-level access is pattern-driven (see the Interfaces page); revoking
//! or banning here is a global override.

use dioxus::prelude::*;
use dioxus_i18n::t;
use plan_ai_design::{Alert, AlertVariant, Badge, BadgeVariant, Button, ButtonVariant, Card};
use uuid::Uuid;

use crate::web::components::ui::{ConfigPanel, PageHeader};
use crate::web::dto::UserAdminView;
use crate::web::server_fns::{
    admin_create_device, admin_delete_user, admin_list_interfaces, admin_list_users,
    admin_set_banned, admin_set_revoked, admin_user_devices, delete_peer, regenerate_peer,
};

#[component]
pub fn Admin() -> Element {
    let mut refresh = use_signal(|| 0u32);
    let users = use_server_future(move || {
        let _ = refresh();
        async move { admin_list_users().await }
    })?;
    let ifaces = use_server_future(admin_list_interfaces)?;
    let mut error = use_signal(|| Option::<String>::None);

    // (id, name) pairs for the per-user device-create interface picker.
    let iface_opts: Vec<(Uuid, String)> = match &*ifaces.read() {
        Some(Ok(l)) => l.iter().map(|i| (i.id, if i.display_name.is_empty() { i.name.clone() } else { i.display_name.clone() })).collect(),
        _ => vec![],
    };

    rsx! {
        PageHeader { eyebrow: t!("nav-users"), title: t!("users-title"), subtitle: t!("users-subtitle") }

        if let Some(e) = error() {
            Alert { variant: AlertVariant::Danger, class: "mb-4", "{e}" }
        }

        match &*users.read() {
            Some(Ok(list)) if list.is_empty() => rsx! { p { class: "text-fg-muted text-sm", {t!("users-empty")} } },
            Some(Ok(list)) => rsx! {
                div { class: "flex flex-col gap-4",
                    for user in list.clone() {
                        UserCard {
                            key: "{user.id}",
                            user: user.clone(),
                            ifaces: iface_opts.clone(),
                            on_change: move |_| refresh.with_mut(|r| *r += 1),
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

#[component]
fn UserCard(
    user: UserAdminView,
    ifaces: Vec<(Uuid, String)>,
    on_change: EventHandler<()>,
    on_error: EventHandler<String>,
) -> Element {
    let uid = user.id;
    let mut local = use_signal(|| 0u32);
    let devices = use_server_future(move || {
        let _ = local();
        async move { admin_user_devices(uid).await }
    })?;
    let mut new_device = use_signal(String::new);
    let mut new_address = use_signal(String::new);
    let mut gen_key = use_signal(|| false);
    let mut created_config = use_signal(|| Option::<(String, String)>::None);
    let mut sel_iface = use_signal(|| ifaces.first().map(|(id, _)| id.to_string()).unwrap_or_default());

    rsx! {
        Card { class: "p-0 overflow-hidden",
            // Header band: identity + status (left), access actions (right).
            div { class: "px-4 py-3 border-b border-line-soft bg-surface-2 flex items-center gap-3 flex-wrap",
                div { class: "flex-1 min-w-0",
                    div { class: "flex items-center gap-2 flex-wrap",
                        span { class: "text-fg-strong font-medium", "{user.email}" }
                        if user.is_admin { Badge { variant: BadgeVariant::Info, {t!("badge-admin")} } }
                        if user.banned { Badge { variant: BadgeVariant::Warn, {t!("badge-banned")} } }
                        if user.access_revoked { Badge { variant: BadgeVariant::Warn, {t!("badge-revoked")} } }
                    }
                    div { class: "text-fg-muted text-xs mt-0.5",
                        {t!("users-device-count-simple", count: user.device_count)}
                        if !user.name.is_empty() { " · {user.name}" }
                    }
                }
                Button {
                    variant: ButtonVariant::Secondary,
                    onclick: move |_| async move {
                        match admin_set_revoked(uid, !user.access_revoked).await {
                            Ok(()) => on_change.call(()),
                            Err(e) => on_error.call(e.to_string()),
                        }
                    },
                    { if user.access_revoked { t!("action-restore-access") } else { t!("action-revoke-access") } }
                }
                Button {
                    variant: ButtonVariant::Secondary,
                    onclick: move |_| async move {
                        match admin_set_banned(uid, !user.banned).await {
                            Ok(()) => on_change.call(()),
                            Err(e) => on_error.call(e.to_string()),
                        }
                    },
                    { if user.banned { t!("action-unban") } else { t!("action-ban") } }
                }
                Button {
                    variant: ButtonVariant::Danger,
                    onclick: move |_| async move {
                        match admin_delete_user(uid).await {
                            Ok(()) => on_change.call(()),
                            Err(e) => on_error.call(e.to_string()),
                        }
                    },
                    {t!("action-delete-user")}
                }
            }

            div { class: "p-4",
                if !ifaces.is_empty() {
                    div { class: "flex items-end gap-2 mb-3 flex-wrap",
                        select {
                            class: "input text-sm w-40",
                            value: "{sel_iface}",
                            onchange: move |e| sel_iface.set(e.value()),
                            for (iid, iname) in ifaces.clone() {
                                option { value: "{iid}", "{iname}" }
                            }
                        }
                        input { class: "input flex-1 text-sm", placeholder: t!("users-new-device-placeholder"),
                            value: "{new_device}", oninput: move |e| new_device.set(e.value()) }
                        input { class: "input w-56 text-sm font-mono", placeholder: t!("users-new-device-subnet-placeholder"),
                            value: "{new_address}", oninput: move |e| new_address.set(e.value()) }
                        label { class: "flex items-center gap-1 text-xs text-fg-muted",
                            input { r#type: "checkbox", checked: gen_key(), onchange: move |e| gen_key.set(e.value() == "true") }
                            {t!("admin-generate-key")}
                        }
                        Button {
                            variant: ButtonVariant::Primary,
                            onclick: move |_| {
                                let (name, addr, g) = (new_device(), new_address(), gen_key());
                                let iid = sel_iface();
                                async move {
                                    let Ok(iid) = Uuid::parse_str(&iid) else { on_error.call("select an interface".into()); return; };
                                    match admin_create_device(iid, uid, name, addr, g).await {
                                        Ok(v) => {
                                            new_device.set(String::new());
                                            new_address.set(String::new());
                                            if let Some(nd) = v { created_config.set(Some((nd.config, nd.qr_svg))); }
                                            local += 1;
                                            on_change.call(());
                                        }
                                        Err(e) => on_error.call(e.to_string()),
                                    }
                                }
                            },
                            {t!("action-add-device")}
                        }
                    }
                    if let Some((config, qr_svg)) = created_config() {
                        ConfigPanel { config, qr_svg }
                    }
                }

                match &*devices.read() {
                    Some(Ok(list)) if list.is_empty() => rsx! { p { class: "text-fg-muted text-xs", {t!("users-no-devices")} } },
                    Some(Ok(list)) => rsx! {
                        div { class: "flex flex-col gap-2",
                            for d in list.clone() {
                                div { key: "{d.id}", class: "flex items-center gap-3 border-t border-line-soft pt-2 first:border-0 first:pt-0",
                                    div { class: "flex-1 min-w-0",
                                        div { class: "flex items-center gap-2",
                                            span { class: "text-fg-strong text-sm", { if d.name.is_empty() { t!("device-unnamed") } else { d.name.clone() } } }
                                            if !d.configured {
                                                Badge { variant: BadgeVariant::Warn, {t!("badge-unconfigured")} }
                                            }
                                        }
                                        div { class: "text-fg-muted text-xs", "{d.interface_name}" }
                                        if d.configured {
                                            div { class: "text-fg-muted text-xs font-mono", "{d.address}" }
                                        } else {
                                            div { class: "text-warn text-xs", {t!("device-unconfigured-note")} }
                                        }
                                    }
                                    DeviceButtons { id: d.id, on_change: move |_| { local += 1; on_change.call(()); }, on_error }
                                }
                            }
                        }
                    },
                    Some(Err(e)) => rsx! { Alert { variant: AlertVariant::Danger, "{e}" } },
                    None => rsx! { p { class: "text-fg-muted text-xs", {t!("users-loading-devices")} } },
                }
            }
        }
    }
}

#[component]
fn DeviceButtons(id: Uuid, on_change: EventHandler<()>, on_error: EventHandler<String>) -> Element {
    let mut config = use_signal(|| Option::<(String, String)>::None);
    rsx! {
        Button {
            variant: ButtonVariant::Secondary,
            onclick: move |_| async move {
                match regenerate_peer(id).await {
                    Ok(v) => { config.set(Some((v.config, v.qr_svg))); on_change.call(()); }
                    Err(e) => on_error.call(e.to_string()),
                }
            },
            {t!("action-regenerate")}
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
        if let Some((cfg, qr)) = config() {
            div { class: "w-full",
                ConfigPanel { config: cfg, qr_svg: qr }
            }
        }
    }
}
