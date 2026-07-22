//! Admin view: every peer on the interface, with create-for-email + delete.

use dioxus::prelude::*;
use plan_ai_design::{Alert, AlertVariant, Button, ButtonVariant, Card};
use uuid::Uuid;

use crate::web::server_fns::{all_peers, create_peer_for, delete_peer, regenerate_peer};

#[component]
pub fn Peers() -> Element {
    let mut refresh = use_signal(|| 0u32);
    let peers = use_server_future(move || {
        let _ = refresh();
        async move { all_peers().await }
    })?;
    let mut email = use_signal(String::new);
    let mut name = use_signal(String::new);
    let mut error = use_signal(|| Option::<String>::None);
    let reload = move || refresh.with_mut(|r| *r += 1);

    let create = move |_| {
        let (e, n) = (email(), name());
        async move {
            match create_peer_for(e, n).await {
                Ok(_) => {
                    email.set(String::new());
                    name.set(String::new());
                    error.set(None);
                    reload();
                }
                Err(err) => error.set(Some(err.to_string())),
            }
        }
    };

    rsx! {
        h2 { class: "text-xl font-semibold text-fg-strong mb-6", "All peers" }

        if let Some(e) = error() {
            Alert { variant: AlertVariant::Danger, class: "mb-4", "{e}" }
        }

        Card { class: "mb-6 p-4 flex items-end gap-3",
            div { class: "flex-1",
                label { class: "label block text-sm text-fg-muted mb-1", "User email" }
                input { class: "input w-full", placeholder: "user@example.com", value: "{email}",
                    oninput: move |e| email.set(e.value()) }
            }
            div { class: "flex-1",
                label { class: "label block text-sm text-fg-muted mb-1", "Name" }
                input { class: "input w-full", placeholder: "laptop", value: "{name}",
                    oninput: move |e| name.set(e.value()) }
            }
            Button { variant: ButtonVariant::Primary, onclick: create, "Add peer" }
        }

        match &*peers.read() {
            Some(Ok(list)) if list.is_empty() => rsx! { p { class: "text-fg-muted text-sm", "No peers yet." } },
            Some(Ok(list)) => rsx! {
                div { class: "flex flex-col gap-2",
                    for peer in list.clone() {
                        Card { key: "{peer.id}", class: "p-3 flex items-center gap-4",
                            div { class: "flex-1 min-w-0",
                                div { class: "text-fg-strong text-sm font-medium",
                                    "{peer.owner_email.clone().unwrap_or_else(|| String::from(\"(unowned)\"))}"
                                }
                                div { class: "text-fg-muted text-xs",
                                    "{peer.name} · {peer.address}"
                                }
                            }
                            AdminActions { id: peer.id, on_change: move |_| reload(), on_error: move |e: String| error.set(Some(e)) }
                        }
                    }
                }
            },
            Some(Err(e)) => rsx! { Alert { variant: AlertVariant::Danger, "{e}" } },
            None => rsx! { p { class: "text-fg-muted", "Loading…" } },
        }
    }
}

#[component]
fn AdminActions(id: Uuid, on_change: EventHandler<()>, on_error: EventHandler<String>) -> Element {
    rsx! {
        Button {
            variant: ButtonVariant::Secondary,
            onclick: move |_| async move {
                match regenerate_peer(id).await {
                    Ok(_) => on_change.call(()),
                    Err(e) => on_error.call(e.to_string()),
                }
            },
            "Regenerate"
        }
        Button {
            variant: ButtonVariant::Danger,
            onclick: move |_| async move {
                match delete_peer(id).await {
                    Ok(()) => on_change.call(()),
                    Err(e) => on_error.call(e.to_string()),
                }
            },
            "Delete"
        }
    }
}
