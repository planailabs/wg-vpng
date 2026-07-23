//! Admin Groups page: reusable named sets of access rules (email patterns +
//! OIDC claim values). Interfaces reference groups; a user belongs to a group if
//! their email matches a pattern or one of their OIDC claim groups is listed.

use dioxus::prelude::*;
use dioxus_i18n::t;
use plan_ai_design::{Alert, AlertVariant, Button, ButtonVariant, Card};
use uuid::Uuid;

use crate::web::components::interfaces::PatternsEditor;
use crate::web::components::ui::PageHeader;
use crate::web::dto::GroupView;
use crate::web::server_fns::{admin_create_group, admin_delete_group, admin_list_groups, admin_update_group};

#[component]
pub fn Groups() -> Element {
    let mut refresh = use_signal(|| 0u32);
    let groups = use_server_future(move || {
        let _ = refresh();
        async move { admin_list_groups().await }
    })?;
    let mut error = use_signal(|| Option::<String>::None);

    rsx! {
        PageHeader { eyebrow: t!("nav-groups"), title: t!("groups-title"), subtitle: t!("groups-subtitle") }

        if let Some(e) = error() {
            Alert { variant: AlertVariant::Danger, class: "mb-4", "{e}" }
        }

        CreateGroup { on_change: move |_| { refresh += 1; }, on_error: move |e: String| error.set(Some(e)) }

        match &*groups.read() {
            Some(Ok(list)) if list.is_empty() => rsx! { p { class: "text-fg-muted text-sm mt-6", {t!("groups-empty")} } },
            Some(Ok(list)) => rsx! {
                div { class: "flex flex-col gap-4 mt-6",
                    for g in list.clone() {
                        GroupCard {
                            key: "{g.id}",
                            group: g.clone(),
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

#[component]
fn CreateGroup(on_change: EventHandler<()>, on_error: EventHandler<String>) -> Element {
    let mut expanded = use_signal(|| false);
    let mut name = use_signal(String::new);
    let mut patterns = use_signal(Vec::<String>::new);
    let mut claim_values = use_signal(Vec::<String>::new);

    let submit = move |_| async move {
        match admin_create_group(name(), patterns(), claim_values()).await {
            Ok(()) => {
                name.set(String::new());
                patterns.set(Vec::new());
                claim_values.set(Vec::new());
                expanded.set(false);
                on_change.call(());
            }
            Err(e) => on_error.call(e.to_string()),
        }
    };

    let header_border = if expanded() { "border-b border-line-soft" } else { "" };
    rsx! {
        Card { class: "p-0 overflow-hidden",
            button {
                class: "w-full px-4 py-3 bg-surface-2 flex items-center gap-2 text-left {header_border}",
                onclick: move |_| expanded.toggle(),
                span { class: "text-sm font-semibold text-fg-strong flex-1", {t!("groups-create-heading")} }
                span { class: "text-fg-muted text-lg leading-none", { if expanded() { "−" } else { "+" } } }
            }
            if expanded() {
                div { class: "p-4 flex flex-col gap-3",
                    div {
                        label { class: "label block text-sm text-fg-muted mb-1", {t!("group-field-name")} }
                        input { class: "input w-full text-sm", placeholder: t!("group-field-name-placeholder"),
                            value: "{name}", oninput: move |e| name.set(e.value()) }
                    }
                    GroupRules { patterns, claim_values }
                    div {
                        Button { variant: ButtonVariant::Primary, onclick: submit, {t!("action-create")} }
                    }
                }
            }
        }
    }
}

/// Shared editor for a group's patterns + claim values.
#[component]
fn GroupRules(patterns: Signal<Vec<String>>, claim_values: Signal<Vec<String>>) -> Element {
    rsx! {
        div {
            label { class: "label text-sm text-fg-muted", {t!("group-field-patterns")} }
            PatternsEditor { patterns }
        }
        div {
            label { class: "label text-sm text-fg-muted", {t!("group-field-claims")} }
            PatternsEditor { patterns: claim_values }
            p { class: "text-fg-faint text-xs", {t!("group-claims-help")} }
        }
    }
}

#[component]
fn GroupCard(group: GroupView, on_change: EventHandler<()>, on_error: EventHandler<String>) -> Element {
    let id = group.id;
    let mut editing = use_signal(|| false);
    let mut name = use_signal(|| group.name.clone());
    let patterns = use_signal(|| group.patterns.clone());
    let claim_values = use_signal(|| group.claim_values.clone());

    let save = move |_| async move {
        match admin_update_group(id, name(), patterns(), claim_values()).await {
            Ok(()) => { editing.set(false); on_change.call(()); }
            Err(e) => on_error.call(e.to_string()),
        }
    };

    rsx! {
        Card { class: "p-4",
            div { class: "flex items-center gap-3 flex-wrap",
                div { class: "flex-1 min-w-0",
                    div { class: "text-fg-strong font-medium", "{group.name}" }
                    div { class: "text-fg-muted text-xs",
                        {t!("group-field-patterns")} ": "
                        { if group.patterns.is_empty() { "—".to_string() } else { group.patterns.join(", ") } }
                        if !group.claim_values.is_empty() {
                            " · "
                            {t!("group-field-claims")} ": {group.claim_values.join(\", \")}"
                        }
                    }
                }
                Button { variant: ButtonVariant::Secondary, onclick: move |_| editing.toggle(),
                    { if editing() { t!("action-cancel") } else { t!("action-edit") } }
                }
                Button {
                    variant: ButtonVariant::Danger,
                    onclick: move |_| async move {
                        match admin_delete_group(id).await {
                            Ok(()) => on_change.call(()),
                            Err(e) => on_error.call(e.to_string()),
                        }
                    },
                    {t!("action-delete")}
                }
            }

            if editing() {
                div { class: "mt-4 border-t border-line-soft pt-3 flex flex-col gap-3",
                    div {
                        label { class: "label block text-sm text-fg-muted mb-1", {t!("group-field-name")} }
                        input { class: "input w-full text-sm", value: "{name}", oninput: move |e| name.set(e.value()) }
                    }
                    GroupRules { patterns, claim_values }
                    div {
                        Button { variant: ButtonVariant::Primary, onclick: save, {t!("action-save")} }
                    }
                }
            }
        }
    }
}
