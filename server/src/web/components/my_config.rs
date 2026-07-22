//! "My VPN" page: a user's own peers with create / show-config / regenerate /
//! delete. Regenerating replaces the peer's private key and re-renders config.

use dioxus::prelude::*;
use dioxus_i18n::t;
use plan_ai_design::{Alert, AlertVariant, Button, ButtonVariant, Card};
use uuid::Uuid;

use crate::web::server_fns::{
    create_my_peer, delete_peer, device_quota, my_peers, peer_config_text, regenerate_peer,
};

#[component]
pub fn MyConfig() -> Element {
    let mut refresh = use_signal(|| 0u32);
    let peers = use_server_future(move || {
        let _ = refresh();
        async move { my_peers().await }
    })?;
    let quota = use_server_future(move || {
        let _ = refresh();
        async move { device_quota().await }
    })?;
    let (used, limit) = match &*quota.read() {
        Some(Ok(q)) => (q.used, q.limit),
        _ => (0, 0),
    };
    let at_limit = used >= limit as i64;
    let mut new_name = use_signal(String::new);
    let mut shown = use_signal(|| Option::<(Uuid, String)>::None);
    let mut error = use_signal(|| Option::<String>::None);

    let create = move |_| {
        let name = new_name();
        async move {
            match create_my_peer(name).await {
                Ok(p) => {
                    new_name.set(String::new());
                    // Immediately surface the fresh config.
                    if let Ok(cfg) = peer_config_text(p.id).await {
                        shown.set(Some((p.id, cfg)));
                    }
                    error.set(None);
                    refresh += 1;
                }
                Err(e) => error.set(Some(e.to_string())),
            }
        }
    };

    rsx! {
        div { class: "mb-8",
            h2 { class: "text-2xl font-semibold text-fg-strong tracking-tight", {t!("devices-title")} }
            p { class: "text-fg-muted text-sm mt-1", {t!("devices-subtitle")} }
        }

        if let Some(e) = error() {
            Alert { variant: AlertVariant::Danger, class: "mb-4", "{e}" }
        }

        Card { class: "mb-6 p-4",
            div { class: "flex items-end gap-3",
                div { class: "flex-1",
                    label { class: "label block text-sm text-fg-muted mb-1", {t!("devices-new-name-label")} }
                    input {
                        class: "input w-full",
                        placeholder: t!("devices-new-name-placeholder"),
                        value: "{new_name}",
                        oninput: move |e| new_name.set(e.value()),
                    }
                }
                Button {
                    variant: ButtonVariant::Primary,
                    disabled: at_limit,
                    onclick: create,
                    {t!("action-generate")}
                }
            }
            p { class: "text-fg-muted text-xs mt-2",
                {t!("devices-quota", used: used, limit: limit)}
                if at_limit {
                    span { class: "text-danger", " " {t!("devices-limit-reached")} }
                }
            }
        }

        match &*peers.read() {
            Some(Ok(list)) if list.is_empty() => rsx! {
                p { class: "text-fg-muted text-sm", {t!("devices-empty")} }
            },
            Some(Ok(list)) => rsx! {
                div { class: "flex flex-col gap-3",
                    for peer in list.clone() {
                        PeerRow {
                            key: "{peer.id}",
                            id: peer.id,
                            name: peer.name.clone(),
                            address: peer.address.clone(),
                            public_key: peer.public_key.clone(),
                            on_change: move |_| { refresh += 1; },
                            on_show: move |cfg: (Uuid, String)| shown.set(Some(cfg)),
                            on_error: move |e: String| error.set(Some(e)),
                        }
                    }
                }
            },
            Some(Err(e)) => rsx! { Alert { variant: AlertVariant::Danger, "{e}" } },
            None => rsx! { p { class: "text-fg-muted", {t!("common-loading")} } },
        }

        if let Some((_, cfg)) = shown() {
            Card { class: "mt-6 p-4",
                div { class: "flex items-center mb-2",
                    h3 { class: "text-sm font-semibold text-fg-strong flex-1", {t!("devices-config-heading")} }
                    Button {
                        variant: ButtonVariant::Secondary,
                        onclick: {
                            let cfg = cfg.clone();
                            move |_| {
                                let cfg = cfg.clone();
                                async move {
                                    let js = format!(
                                        "navigator.clipboard && navigator.clipboard.writeText({});",
                                        serde_json::to_string(&cfg).unwrap_or_default()
                                    );
                                    let _ = document::eval(&js);
                                }
                            }
                        },
                        {t!("action-copy")}
                    }
                }
                pre { class: "text-xs bg-surface-2 rounded-md p-3 overflow-x-auto whitespace-pre", "{cfg}" }
            }
        }
    }
}

#[component]
fn PeerRow(
    id: Uuid,
    name: String,
    address: String,
    public_key: String,
    on_change: EventHandler<()>,
    on_show: EventHandler<(Uuid, String)>,
    on_error: EventHandler<String>,
) -> Element {
    let short_key: String = public_key.chars().take(16).collect();
    let display_name = if name.is_empty() { t!("device-unnamed") } else { name };

    rsx! {
        Card { class: "p-4 flex items-center gap-4",
            div { class: "flex-1 min-w-0",
                div { class: "text-fg-strong font-medium", "{display_name}" }
                div { class: "text-fg-muted text-xs", "{address} · {short_key}…" }
            }
            Button {
                variant: ButtonVariant::Secondary,
                onclick: move |_| async move {
                    match peer_config_text(id).await {
                        Ok(cfg) => on_show.call((id, cfg)),
                        Err(e) => on_error.call(e.to_string()),
                    }
                },
                {t!("action-show-config")}
            }
            Button {
                variant: ButtonVariant::Secondary,
                onclick: move |_| async move {
                    match regenerate_peer(id).await {
                        Ok(_) => {
                            match peer_config_text(id).await {
                                Ok(cfg) => on_show.call((id, cfg)),
                                Err(e) => on_error.call(e.to_string()),
                            }
                            on_change.call(());
                        }
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
        }
    }
}
