//! App shell: a full-width top bar with a centered inner row (brand + tab nav +
//! user controls) over a centered, max-width content column. Deliberately
//! enterprise-flavoured — top navigation, generous whitespace, a single
//! focused content column rather than a wide dashboard sprawl.

use dioxus::prelude::*;
use dioxus_i18n::t;
use plan_ai_design::{LanguagePicker, ThemeToggle};

use crate::web::app::Route;
use crate::web::server_fns::get_current_user;

#[component]
pub fn Layout() -> Element {
    let user = use_server_future(get_current_user)?;
    let (name, email, is_admin) = match &*user.read() {
        Some(Ok(u)) => (u.name.clone(), u.email.clone(), u.is_admin),
        _ => (String::new(), String::new(), false),
    };
    let display = if name.is_empty() { email.clone() } else { name };
    let initial = display
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string());

    rsx! {
        div { class: "min-h-screen h-dvh w-full flex flex-col bg-bg overflow-hidden",

            // ── Top bar ──────────────────────────────────────────────
            header { class: "shrink-0 border-b border-line bg-surface",
                div { class: "max-w-6xl mx-auto px-6 h-16 flex items-center gap-8",

                    // Brand mark
                    div { class: "flex items-center gap-2.5 shrink-0",
                        div {
                            class: "h-9 w-9 rounded-lg bg-brand text-fg-invert flex items-center justify-center font-bold text-sm shadow-card",
                            "wg"
                        }
                        div { class: "leading-tight hidden sm:block",
                            div { class: "font-semibold text-fg-strong text-sm", {t!("brand-title")} }
                            div { class: "text-fg-faint text-[11px] tracking-wide uppercase", {t!("brand-subtitle")} }
                        }
                    }

                    // Primary navigation (tabs)
                    nav { class: "flex items-stretch gap-1 h-full",
                        NavTab { to: Route::MyConfig {}, label: t!("nav-devices") }
                        if is_admin {
                            NavTab { to: Route::Interfaces {}, label: t!("nav-interface") }
                            NavTab { to: Route::Admin {}, label: t!("nav-users") }
                        }
                    }

                    div { class: "flex-1" }

                    // User controls
                    div { class: "flex items-center gap-2",
                        LanguagePicker {}
                        ThemeToggle {}
                        div { class: "w-px h-6 bg-line mx-1" }
                        if !display.is_empty() {
                            div { class: "flex items-center gap-2",
                                div {
                                    class: "h-8 w-8 rounded-full bg-surface-3 text-fg-strong flex items-center justify-center text-xs font-semibold",
                                    "{initial}"
                                }
                                span { class: "text-sm text-fg-muted hidden md:inline max-w-[12rem] truncate", "{display}" }
                            }
                        }
                        a {
                            class: "text-sm text-fg-muted hover:text-danger px-2 py-1 rounded-md transition-colors",
                            href: "/auth/logout",
                            title: t!("action-sign-out"),
                            {t!("action-sign-out")}
                        }
                    }
                }
            }

            // ── Centered content column ─────────────────────────────
            main { class: "flex-1 min-w-0 overflow-y-auto",
                div { class: "max-w-5xl mx-auto px-6 py-10 w-full",
                    SuspenseBoundary {
                        fallback: |_| rsx! {
                            div { class: "flex items-center justify-center py-24 text-fg-muted text-sm", {t!("common-loading")} }
                        },
                        Outlet::<Route> {}
                    }
                }
            }
        }
    }
}

#[component]
fn NavTab(to: Route, label: String) -> Element {
    rsx! {
        Link {
            to,
            class: "flex items-center px-3 text-sm font-medium text-fg-muted hover:text-fg-strong border-b-2 border-transparent -mb-px transition-colors",
            active_class: "!text-brand !border-brand",
            "{label}"
        }
    }
}
