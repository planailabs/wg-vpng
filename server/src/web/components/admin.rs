//! Admin page: manage every user's devices (everything a user can do, per
//! user) plus access control — set device limits, revoke access, ban, delete.

use dioxus::prelude::*;
use dioxus_i18n::t;
use plan_ai_design::{Alert, AlertVariant, Badge, BadgeVariant, Button, ButtonVariant, Card};
use uuid::Uuid;

use crate::web::dto::UserAdminView;
use crate::web::server_fns::{
    admin_create_device, admin_delete_user, admin_list_users, admin_set_banned,
    admin_set_device_limit, admin_set_revoked, admin_user_devices, delete_peer, peer_config_text,
    regenerate_peer,
};

#[component]
pub fn Admin() -> Element {
    let mut refresh = use_signal(|| 0u32);
    let users = use_server_future(move || {
        let _ = refresh();
        async move { admin_list_users().await }
    })?;
    let mut error = use_signal(|| Option::<String>::None);

    rsx! {
        div { class: "mb-8",
            h2 { class: "text-2xl font-semibold text-fg-strong tracking-tight", {t!("users-title")} }
            p { class: "text-fg-muted text-sm mt-1", {t!("users-subtitle")} }
        }

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
fn UserCard(user: UserAdminView, on_change: EventHandler<()>, on_error: EventHandler<String>) -> Element {
    let uid = user.id;
    let mut local = use_signal(|| 0u32);
    let devices = use_server_future(move || {
        let _ = local();
        async move { admin_user_devices(uid).await }
    })?;
    let mut new_device = use_signal(String::new);
    let mut new_address = use_signal(String::new);
    let mut limit_input = use_signal(|| user.device_limit.map(|l| l.to_string()).unwrap_or_default());

    rsx! {
        Card { class: "p-4",
            div { class: "flex items-center gap-3 flex-wrap",
                div { class: "flex-1 min-w-0",
                    div { class: "text-fg-strong font-medium", "{user.email}" }
                    div { class: "text-fg-muted text-xs",
                        {t!("users-device-count", count: user.device_count, limit: user.effective_limit)}
                        if !user.name.is_empty() { " · {user.name}" }
                    }
                }
                if user.is_admin { Badge { variant: BadgeVariant::Info, {t!("badge-admin")} } }
                if user.banned { Badge { variant: BadgeVariant::Warn, {t!("badge-banned")} } }
                if user.access_revoked { Badge { variant: BadgeVariant::Warn, {t!("badge-revoked")} } }
            }

            // Access controls.
            div { class: "flex items-center gap-2 flex-wrap mt-3",
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
                div { class: "flex items-center gap-1 ml-auto",
                    label { class: "text-fg-muted text-xs", {t!("users-limit-label")} }
                    input {
                        class: "input w-16 text-sm",
                        r#type: "number",
                        placeholder: t!("users-limit-placeholder"),
                        value: "{limit_input}",
                        oninput: move |e| limit_input.set(e.value()),
                    }
                    Button {
                        variant: ButtonVariant::Secondary,
                        onclick: move |_| async move {
                            let parsed = limit_input().trim().parse::<i32>().ok();
                            match admin_set_device_limit(uid, parsed).await {
                                Ok(()) => on_change.call(()),
                                Err(e) => on_error.call(e.to_string()),
                            }
                        },
                        {t!("action-set")}
                    }
                }
            }

            // Devices.
            div { class: "mt-4 border-t border-line-soft pt-3",
                div { class: "flex items-end gap-2 mb-2",
                    input {
                        class: "input flex-1 text-sm",
                        placeholder: t!("users-new-device-placeholder"),
                        value: "{new_device}",
                        oninput: move |e| new_device.set(e.value()),
                    }
                    input {
                        class: "input w-56 text-sm font-mono",
                        placeholder: t!("users-new-device-subnet-placeholder"),
                        value: "{new_address}",
                        oninput: move |e| new_address.set(e.value()),
                    }
                    Button {
                        variant: ButtonVariant::Primary,
                        onclick: move |_| {
                            let (name, addr) = (new_device(), new_address());
                            async move {
                                match admin_create_device(uid, name, addr).await {
                                    Ok(_) => {
                                        new_device.set(String::new());
                                        new_address.set(String::new());
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

                match &*devices.read() {
                    Some(Ok(list)) if list.is_empty() => rsx! { p { class: "text-fg-muted text-xs", {t!("users-no-devices")} } },
                    Some(Ok(list)) => rsx! {
                        div { class: "flex flex-col gap-1",
                            for d in list.clone() {
                                div { key: "{d.id}", class: "flex items-center gap-2 text-sm",
                                    span { class: "flex-1 min-w-0 truncate text-fg",
                                        "{d.name} · {d.address}"
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
    let mut config = use_signal(|| Option::<String>::None);
    rsx! {
        Button {
            variant: ButtonVariant::Secondary,
            onclick: move |_| async move {
                match peer_config_text(id).await {
                    Ok(c) => config.set(Some(c)),
                    Err(e) => on_error.call(e.to_string()),
                }
            },
            {t!("action-config")}
        }
        Button {
            variant: ButtonVariant::Secondary,
            onclick: move |_| async move {
                match regenerate_peer(id).await {
                    Ok(_) => on_change.call(()),
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
        if let Some(c) = config() {
            div { class: "w-full",
                pre { class: "text-xs bg-surface-2 rounded-md p-2 mt-1 overflow-x-auto whitespace-pre", "{c}" }
            }
        }
    }
}
