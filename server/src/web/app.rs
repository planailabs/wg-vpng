//! Dioxus fullstack app: route table + root component.

use dioxus::prelude::*;
use dioxus_i18n::prelude::*;
use dioxus_i18n::unic_langid::langid;

use super::components::admin::Admin;
use super::components::interfaces::Interfaces;
use super::components::layout::Layout;
use super::components::my_config::MyConfig;

#[derive(Debug, Clone, Routable, PartialEq)]
pub enum Route {
    #[layout(Layout)]
    #[route("/")]
    MyConfig {},
    #[route("/interfaces")]
    Interfaces {},
    #[route("/admin")]
    Admin {},
}

#[component]
pub fn App() -> Element {
    let mut i18n = use_init_i18n(|| {
        // Concatenate shared (plan-ai-design) + app-specific translations, the
        // same way mac-mgmt does.
        let en: &'static str = Box::leak(
            format!("{}\n{}", plan_ai_design::i18n::EN_US, include_str!("./en-US.ftl")).into_boxed_str(),
        );
        let de: &'static str = Box::leak(
            format!("{}\n{}", plan_ai_design::i18n::DE_DE, include_str!("./de-DE.ftl")).into_boxed_str(),
        );
        I18nConfig::new(langid!("en-US"))
            .with_locale(Locale::new_static(langid!("en-US"), en))
            .with_locale(Locale::new_static(langid!("de-DE"), de))
    });
    let css_href = format!("/tailwind.css?v={}", env!("BUILD_TIMESTAMP"));

    // Restore language preference from localStorage.
    use_effect(move || {
        spawn(async move {
            let result =
                document::eval("try { return localStorage.getItem('lang') || ''; } catch(e) { return ''; }").await;
            if let Ok(val) = result {
                if val.as_str() == Some("de-DE") {
                    let _ = i18n.set_language(langid!("de-DE"));
                }
            }
        });
    });

    // Remove the pre-hydration banner once WASM has hydrated.
    use_effect(|| {
        document::eval("document.getElementById('wasm-loading')?.remove();");
    });

    rsx! {
        script { dangerous_inner_html: plan_ai_design::THEME_INIT_SCRIPT }
        document::Link { rel: "stylesheet", href: "{css_href}" }

        div {
            id: "wasm-loading",
            style: plan_ai_design::WASM_LOADING_STYLE,
            dangerous_inner_html: plan_ai_design::WASM_LOADING_INNER,
        }

        Router::<Route> {}
    }
}
