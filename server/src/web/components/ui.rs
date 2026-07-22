//! Small shared UI building blocks for a consistent, enterprise-flavoured look.

use dioxus::prelude::*;
use dioxus_i18n::t;
use plan_ai_design::{Button, ButtonVariant, Card};

/// Page header: a small brand eyebrow, a large title, a subtitle, and a rule —
/// used at the top of every page for a consistent, structured feel.
#[component]
pub fn PageHeader(eyebrow: String, title: String, subtitle: String) -> Element {
    rsx! {
        div { class: "mb-8 pb-4 border-b border-line",
            if !eyebrow.is_empty() {
                div { class: "text-brand text-[11px] font-semibold uppercase tracking-[0.14em] mb-1.5", "{eyebrow}" }
            }
            h2 { class: "text-2xl font-semibold text-fg-strong tracking-tight", "{title}" }
            if !subtitle.is_empty() {
                p { class: "text-fg-muted text-sm mt-1.5 max-w-2xl leading-relaxed", "{subtitle}" }
            }
        }
    }
}

/// A titled section card with an optional right-aligned actions slot.
#[component]
pub fn SectionCard(title: String, children: Element) -> Element {
    rsx! {
        Card { class: "p-0 overflow-hidden",
            div { class: "px-4 py-3 border-b border-line-soft bg-surface-2",
                h3 { class: "text-sm font-semibold text-fg-strong", "{title}" }
            }
            div { class: "p-4", {children} }
        }
    }
}

/// The freshly-generated config: QR (for the WireGuard mobile app), the config
/// text, and Copy + Download. Shown once — the private key is not stored.
#[component]
pub fn ConfigPanel(config: String, qr_svg: String) -> Element {
    let copy_cfg = config.clone();
    let dl_cfg = config.clone();
    rsx! {
        Card { class: "mt-6 p-4 border-brand-soft",
            div { class: "flex flex-col sm:flex-row gap-4",
                if !qr_svg.is_empty() {
                    div {
                        // The SVG carries its own width/height; the viewBox lets us
                        // scale it to fill this fixed box via the child selector.
                        class: "shrink-0 rounded-lg bg-white p-3 self-start shadow-card [&>svg]:block [&>svg]:w-full [&>svg]:h-full",
                        style: "width:240px;height:240px",
                        dangerous_inner_html: "{qr_svg}",
                    }
                }
                div { class: "flex-1 min-w-0",
                    div { class: "flex items-center gap-2 mb-2",
                        h3 { class: "text-sm font-semibold text-fg-strong flex-1", {t!("devices-config-heading")} }
                        Button {
                            variant: ButtonVariant::Secondary,
                            onclick: move |_| {
                                let c = copy_cfg.clone();
                                async move {
                                    let js = format!(
                                        "navigator.clipboard && navigator.clipboard.writeText({});",
                                        serde_json::to_string(&c).unwrap_or_default()
                                    );
                                    let _ = document::eval(&js);
                                }
                            },
                            {t!("action-copy")}
                        }
                        Button {
                            variant: ButtonVariant::Secondary,
                            onclick: move |_| {
                                let c = dl_cfg.clone();
                                async move {
                                    // Trigger a .conf download via a Blob + anchor click.
                                    let js = format!(
                                        "(function(){{const b=new Blob([{}],{{type:'text/plain'}});\
                                          const u=URL.createObjectURL(b);const a=document.createElement('a');\
                                          a.href=u;a.download='wireguard.conf';document.body.appendChild(a);\
                                          a.click();a.remove();URL.revokeObjectURL(u);}})();",
                                        serde_json::to_string(&c).unwrap_or_default()
                                    );
                                    let _ = document::eval(&js);
                                }
                            },
                            {t!("action-download")}
                        }
                    }
                    pre { class: "text-xs bg-surface-2 rounded-md p-3 overflow-x-auto whitespace-pre", "{config}" }
                    p { class: "text-fg-faint text-xs mt-2", {t!("devices-scan-hint")} }
                }
            }
        }
    }
}
