//! App shell: sidebar nav + topbar (theme/language/logout) around the routed
//! page outlet.

use dioxus::prelude::*;
use plan_ai_design::{LanguagePicker, ThemeToggle};

use crate::web::app::Route;
use crate::web::server_fns::get_current_user;

#[component]
pub fn Layout() -> Element {
    let user = use_server_future(get_current_user)?;
    let (name, is_admin) = match &*user.read() {
        Some(Ok(u)) => (u.name.clone(), u.is_admin),
        _ => (String::new(), false),
    };
    let display = if name.is_empty() { "".to_string() } else { name };

    rsx! {
        div { class: "h-screen h-dvh w-full flex overflow-hidden",
            nav { class: "shrink-0 w-56 border-r border-line bg-surface flex flex-col p-4 gap-1",
                div { class: "text-lg font-semibold text-fg-strong mb-4", "wg-vpng" }
                NavLink { to: Route::MyConfig {}, label: "My devices" }
                if is_admin {
                    NavLink { to: Route::Interfaces {}, label: "Interface" }
                    NavLink { to: Route::Admin {}, label: "Users" }
                }
            }

            div { class: "flex-1 flex flex-col min-w-0 overflow-hidden",
                header { class: "shrink-0 h-14 border-b border-line flex items-center px-6 gap-3",
                    h1 { class: "text-base font-semibold text-fg", "WireGuard VPN generator" }
                    div { class: "flex-1" }
                    LanguagePicker {}
                    ThemeToggle {}
                    if !display.is_empty() {
                        span { class: "text-sm text-fg-muted ml-2", "{display}" }
                    }
                    a {
                        class: "text-sm text-fg-muted hover:text-danger ml-2",
                        href: "/auth/logout",
                        "Logout"
                    }
                }

                main { class: "flex-1 min-w-0 overflow-y-auto p-4 sm:p-6 lg:p-8",
                    SuspenseBoundary {
                        fallback: |_| rsx! { div { class: "text-fg-muted", "Loading…" } },
                        Outlet::<Route> {}
                    }
                }
            }
        }
    }
}

#[component]
fn NavLink(to: Route, label: &'static str) -> Element {
    rsx! {
        Link {
            to,
            class: "px-3 py-2 rounded-md text-sm text-fg hover:bg-surface-2",
            active_class: "bg-surface-2 text-fg-strong font-medium",
            "{label}"
        }
    }
}
