use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_meta::*;
use leptos_router::{
    components::{Route, Router, Routes, A},
    path,
};

mod api;
mod i18n;
mod pages;
mod recs;

use i18n::{dict, Lang};

#[derive(Clone, Debug)]
pub struct AuthState {
    pub token: String,
    pub user_id: String,
    pub email: String,
    pub username: Option<String>,
    pub is_admin: bool,
}

/// Decode the `is_admin` claim from a JWT without a crypto library.
/// JWTs are header.payload.signature — the payload is base64url JSON.
pub fn jwt_is_admin(token: &str) -> bool {
    let payload = match token.split('.').nth(1) {
        Some(p) => p,
        None => return false,
    };
    // base64url → base64: replace URL-safe chars and add padding
    let b64 = payload.replace('-', "+").replace('_', "/");
    let pad = (4 - b64.len() % 4) % 4;
    let b64 = format!("{}{}", b64, "=".repeat(pad));

    let decoded = web_sys::window()
        .and_then(|w| w.atob(&b64).ok())
        .unwrap_or_default();

    serde_json::from_str::<serde_json::Value>(&decoded)
        .ok()
        .and_then(|v| v.get("is_admin").and_then(|b| b.as_bool()))
        .unwrap_or(false)
}

const LS_TOKEN: &str = "gf_token";
const LS_USER_ID: &str = "gf_user_id";
const LS_EMAIL: &str = "gf_email";
const LS_USERNAME: &str = "gf_username";

const LS_WATCH_REGION: &str = "gf_watch_region";
const LS_WATCH_PROVIDERS_US: &str = "gf_watch_providers_us";
const LS_WATCH_PROVIDERS_BR: &str = "gf_watch_providers_br";
const LS_WATCH_RENTALS: &str = "gf_watch_rentals";

pub fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()
        .and_then(|w| w.local_storage().ok())
        .and_then(|s| s)
}

/// Anonymous-first "What can I watch?" preferences, persisted to localStorage.
/// `region` is "US" or "BR"; provider ids are TMDB ids per region.
#[derive(Clone, PartialEq, Debug)]
pub struct WatchPrefs {
    pub region: String,
    pub providers_us: Vec<i32>,
    pub providers_br: Vec<i32>,
    pub rentals: bool,
}

impl WatchPrefs {
    /// Provider ids selected for the active region.
    pub fn selected(&self) -> &Vec<i32> {
        if self.region == "BR" {
            &self.providers_br
        } else {
            &self.providers_us
        }
    }

    /// Comma-separated selected ids for the active region (for query params).
    pub fn selected_csv(&self) -> String {
        self.selected()
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(",")
    }

    /// Is any filtering active (a service selected, or rentals toggled on)?
    pub fn active(&self) -> bool {
        !self.selected().is_empty() || self.rentals
    }

    /// Toggle a provider id for the active region.
    pub fn toggle(&mut self, provider_id: i32) {
        let list = if self.region == "BR" {
            &mut self.providers_br
        } else {
            &mut self.providers_us
        };
        if let Some(pos) = list.iter().position(|x| *x == provider_id) {
            list.remove(pos);
        } else {
            list.push(provider_id);
        }
    }

    /// Clear all selections and the rentals toggle for the active region.
    pub fn clear(&mut self) {
        if self.region == "BR" {
            self.providers_br.clear();
        } else {
            self.providers_us.clear();
        }
        self.rentals = false;
    }
}

fn parse_ids_csv(s: &str) -> Vec<i32> {
    s.split(',')
        .filter_map(|x| x.trim().parse::<i32>().ok())
        .collect()
}

/// Load watch prefs from localStorage, defaulting the region by language when unset.
pub fn load_watch_prefs(default_region: &str) -> WatchPrefs {
    let ls = local_storage();
    let get = |k: &str| ls.as_ref().and_then(|s| s.get_item(k).ok().flatten());
    let region = get(LS_WATCH_REGION).unwrap_or_else(|| default_region.to_string());
    let region = if region == "BR" { "BR" } else { "US" }.to_string();
    WatchPrefs {
        region,
        providers_us: get(LS_WATCH_PROVIDERS_US)
            .map(|s| parse_ids_csv(&s))
            .unwrap_or_default(),
        providers_br: get(LS_WATCH_PROVIDERS_BR)
            .map(|s| parse_ids_csv(&s))
            .unwrap_or_default(),
        rentals: get(LS_WATCH_RENTALS).as_deref() == Some("1"),
    }
}

/// Access the app-wide watch-prefs signal from context.
pub fn use_watch() -> RwSignal<WatchPrefs> {
    use_context::<RwSignal<WatchPrefs>>().expect("WatchPrefs context missing — provide it in App")
}

/// Persist watch prefs to localStorage.
pub fn save_watch_prefs(p: &WatchPrefs) {
    if let Some(ls) = local_storage() {
        let csv = |v: &[i32]| {
            v.iter()
                .map(|x| x.to_string())
                .collect::<Vec<_>>()
                .join(",")
        };
        let _ = ls.set_item(LS_WATCH_REGION, &p.region);
        let _ = ls.set_item(LS_WATCH_PROVIDERS_US, &csv(&p.providers_us));
        let _ = ls.set_item(LS_WATCH_PROVIDERS_BR, &csv(&p.providers_br));
        let _ = ls.set_item(LS_WATCH_RENTALS, if p.rentals { "1" } else { "0" });
    }
}

pub fn save_auth_to_storage(token: &str, user_id: &str, email: &str, username: Option<&str>) {
    if let Some(ls) = local_storage() {
        let _ = ls.set_item(LS_TOKEN, token);
        let _ = ls.set_item(LS_USER_ID, user_id);
        let _ = ls.set_item(LS_EMAIL, email);
        if let Some(u) = username {
            let _ = ls.set_item(LS_USERNAME, u);
        } else {
            let _ = ls.remove_item(LS_USERNAME);
        }
    }
}

pub fn load_auth_from_storage() -> Option<AuthState> {
    let ls = local_storage()?;
    let token = ls.get_item(LS_TOKEN).ok()??;
    let user_id = ls.get_item(LS_USER_ID).ok()??;
    let email = ls.get_item(LS_EMAIL).ok()??;
    let username = ls.get_item(LS_USERNAME).ok().flatten();
    if token.is_empty() {
        return None;
    }
    let is_admin = jwt_is_admin(&token);
    Some(AuthState {
        token,
        user_id,
        email,
        username,
        is_admin,
    })
}

pub fn logout(auth: RwSignal<Option<AuthState>>) {
    if let Some(ls) = local_storage() {
        let _ = ls.remove_item(LS_TOKEN);
        let _ = ls.remove_item(LS_USER_ID);
        let _ = ls.remove_item(LS_EMAIL);
        let _ = ls.remove_item(LS_USERNAME);
    }
    auth.set(None);
}

fn main() {
    console_error_panic_hook::set_once();
    if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
        if let Some(el) = doc.get_element_by_id("app-loading") {
            el.remove();
        }
    }
    mount_to_body(|| view! { <App /> })
}

#[component]
fn App() -> impl IntoView {
    provide_meta_context();

    let auth: RwSignal<Option<AuthState>> = RwSignal::new(load_auth_from_storage());
    provide_context(auth);

    // Locale: default from browser/localStorage, persisted on change, and mirrored
    // onto <html lang> for accessibility/SEO.
    let lang: RwSignal<Lang> = RwSignal::new(i18n::detect_lang());
    provide_context(lang);
    Effect::new(move |_| {
        let l = lang.get();
        i18n::save_lang(l);
        if let Some(el) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.document_element())
        {
            let _ = el.set_attribute("lang", l.html_tag());
        }
    });

    // "What can I watch?" prefs: anonymous-first, region default follows language
    // (PT→BR, EN→US), persisted to localStorage on change. List pages read this
    // context so they refetch when the user changes providers/region/rentals.
    let default_region = if lang.get_untracked() == Lang::Pt {
        "BR"
    } else {
        "US"
    };
    let watch: RwSignal<WatchPrefs> = RwSignal::new(load_watch_prefs(default_region));
    provide_context(watch);
    Effect::new(move |_| {
        let p = watch.get();
        save_watch_prefs(&p);
    });

    // Keep the watch region locked to the UI language (PT→BR, EN→US) so displayed
    // providers always match what the user can read. Overwrites the persisted
    // region on every language change.
    Effect::new(move |_| {
        let region = if lang.get() == Lang::Pt { "BR" } else { "US" };
        watch.update(|w| {
            if w.region != region {
                w.region = region.to_string();
            }
        });
    });

    // Cross-device sync (Phase 10 Batch 6): when signed in, hydrate from the server
    // if local selections are empty, and push on every change. Last-write-wins.
    if let Some(a) = auth.get_untracked() {
        let local_empty = watch.with_untracked(|w| {
            w.providers_us.is_empty() && w.providers_br.is_empty() && !w.rentals
        });
        if local_empty {
            let token = a.token.clone();
            spawn_local(async move {
                if let Ok(p) = api::get_user_providers(&token).await {
                    if !p.us.is_empty() || !p.br.is_empty() {
                        watch.update(|w| {
                            w.providers_us = p.us;
                            w.providers_br = p.br;
                        });
                    }
                }
            });
        }
    }
    Effect::new(move |prev: Option<()>| {
        let p = watch.get(); // track changes
                             // Skip the initial run so we don't overwrite the server with the local
                             // default before hydration has a chance to run.
        if prev.is_some() {
            if let Some(a) = auth.get_untracked() {
                let token = a.token.clone();
                let payload = gem_finder_shared::types::UserProvidersPayload {
                    us: p.providers_us.clone(),
                    br: p.providers_br.clone(),
                };
                spawn_local(async move {
                    let _ = api::put_user_providers(payload, &token).await;
                });
            }
        }
    });

    let smtp_ok: RwSignal<bool> = RwSignal::new(true);
    provide_context(smtp_ok);

    // Nav badge for unread recommendations (Ethos C1). One fetch per app
    // load when signed in — no polling, badge-only notification per spec.
    // RecsPage decrements this directly via context after marking reads.
    let unread_recs: RwSignal<i64> = RwSignal::new(0);
    provide_context(unread_recs);
    if let Some(a) = auth.get_untracked() {
        spawn_local(async move {
            if let Ok(count) = api::fetch_unread_count(&a.token).await {
                unread_recs.set(count);
            }
        });
    }

    spawn_local(async move {
        if let Ok(status) = api::fetch_admin_status().await {
            let configured = status
                .get("smtp_configured")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            smtp_ok.set(configured);
        }
    });

    // Current dictionary; `d()` re-reads the locale signal so strings swap reactively.
    let d = move || dict(lang.get());

    view! {
        <Router>
            <Title text="Pedrilha" />
            <Meta name="description" content=move || d().meta_description />

            <div class="min-h-screen bg-sc-base text-stone-200">
                <header class="border-b border-sc-border sticky top-0 z-[60]" style="background-color: rgba(23,16,10,0.88); backdrop-filter: blur(10px); -webkit-backdrop-filter: blur(10px);">
                    <nav class="max-w-7xl mx-auto px-4 py-4 flex flex-col sm:flex-row sm:justify-between items-center gap-3 sm:gap-0">
                        <A href="/" attr:class="font-display text-2xl sm:text-3xl tracking-widest text-stone-100 hover:text-sc-accent transition-colors">
                            "PEDRILHA"
                        </A>
                        <div class="flex flex-wrap justify-center gap-x-4 gap-y-1 sm:gap-6 items-center">
                            <A href="/" attr:class="text-stone-400 hover:text-stone-100 transition-colors text-xs sm:text-sm tracking-wide aria-[current=page]:text-sc-accent aria-[current=page]:border-b-2 aria-[current=page]:border-sc-accent">
                                {move || d().nav_gems}
                            </A>
                            <A href="/acclaimed" attr:class="text-stone-400 hover:text-stone-100 transition-colors text-xs sm:text-sm tracking-wide aria-[current=page]:text-sc-accent aria-[current=page]:border-b-2 aria-[current=page]:border-sc-accent">
                                {move || d().nav_acclaimed}
                            </A>
                            <A href="/wildcards" attr:class="text-stone-400 hover:text-stone-100 transition-colors text-xs sm:text-sm tracking-wide aria-[current=page]:text-sc-accent aria-[current=page]:border-b-2 aria-[current=page]:border-sc-accent">
                                {move || d().nav_wildcards}
                            </A>
                            {move || auth.get().filter(|a| a.is_admin).map(|_| view! {
                                <A href="/admin" attr:class="text-stone-400 hover:text-stone-100 transition-colors text-xs sm:text-sm tracking-wide aria-[current=page]:text-sc-accent aria-[current=page]:border-b-2 aria-[current=page]:border-sc-accent">
                                    {move || d().nav_admin}
                                </A>
                            })}
                            {move || match auth.get() {
                                Some(a) => {
                                    let display = a.username.clone()
                                        .unwrap_or_else(|| {
                                            a.email.split('@').next().unwrap_or("user").to_string()
                                        });
                                    // Account menu — declutters the header by tucking
                                    // Recommendations / Watchlist / Sign out behind the
                                    // username. Unread-rec badge surfaces on the trigger
                                    // so notifications stay visible while collapsed.
                                    let menu_open = RwSignal::new(false);
                                    let close_menu = move |_: web_sys::MouseEvent| menu_open.set(false);
                                    view! {
                                        <div class="relative">
                                            <button
                                                class="flex items-center gap-1 text-stone-300 hover:text-stone-100 transition-colors text-xs sm:text-sm tracking-wide"
                                                on:click=move |_| menu_open.update(|o| *o = !*o)
                                            >
                                                <span>{display}</span>
                                                {move || (unread_recs.get() > 0).then(|| view! {
                                                    <span class="px-1.5 rounded-full bg-sc-accent text-stone-900 text-[10px] font-bold">
                                                        {move || unread_recs.get()}
                                                    </span>
                                                })}
                                                <span class="text-stone-500 text-[10px]" aria-hidden="true">"▾"</span>
                                            </button>
                                            {move || menu_open.get().then(|| view! {
                                                // Click-outside catcher — closes the menu.
                                                <div class="fixed inset-0 z-[65]" on:click=close_menu></div>
                                                <div class="absolute right-0 mt-2 py-1 min-w-[190px] bg-sc-panel border border-sc-border rounded shadow-xl z-[70] flex flex-col">
                                                    <A href="/recs" on:click=close_menu attr:class="flex items-center justify-between px-4 py-2 text-stone-300 hover:text-stone-100 hover:bg-sc-card transition-colors text-xs sm:text-sm tracking-wide">
                                                        <span>{move || d().nav_recs}</span>
                                                        {move || (unread_recs.get() > 0).then(|| view! {
                                                            <span class="ml-2 px-1.5 rounded-full bg-sc-accent text-stone-900 text-[10px] font-bold">
                                                                {move || unread_recs.get()}
                                                            </span>
                                                        })}
                                                    </A>
                                                    <A href="/watchlist" on:click=close_menu attr:class="px-4 py-2 text-stone-300 hover:text-stone-100 hover:bg-sc-card transition-colors text-xs sm:text-sm tracking-wide">
                                                        {move || d().nav_watchlist}
                                                    </A>
                                                    <button
                                                        class="text-left px-4 py-2 text-stone-400 hover:text-stone-100 hover:bg-sc-card transition-colors text-xs sm:text-sm tracking-wide"
                                                        on:click=move |_| { menu_open.set(false); logout(auth); }
                                                    >
                                                        {move || d().nav_signout}
                                                    </button>
                                                </div>
                                            })}
                                        </div>
                                    }.into_any()
                                }
                                None => {
                                    if smtp_ok.get() {
                                        view! {
                                            <A href="/signin" attr:class="text-stone-400 hover:text-stone-100 transition-colors text-xs sm:text-sm tracking-wide">
                                                {move || d().nav_signin}
                                            </A>
                                        }.into_any()
                                    } else {
                                        view! { <span /> }.into_any()
                                    }
                                }
                            }}
                            <LocaleToggle />
                        </div>
                    </nav>
                </header>

                <main>
                    <Routes fallback=move || view! {
                        <div class="max-w-7xl mx-auto px-4 py-24 text-center">
                            <Title text=move || d().nf_meta_title />
                            <h1 class="font-display text-4xl sm:text-6xl tracking-wide text-stone-100 mb-4">{move || d().nf_title}</h1>
                            <p class="text-stone-400 mb-8">{move || d().nf_body}</p>
                            <A href="/" attr:class="text-sc-accent hover:text-sc-accent-hover">{move || d().nf_back}</A>
                        </div>
                    }>
                        <Route path=path!("/") view=pages::HomePage />
                        <Route path=path!("/acclaimed") view=pages::AcclaimedPage />
                        <Route path=path!("/wildcards") view=pages::WildcardsPage />
                        <Route path=path!("/movie/:id") view=pages::MovieDetail />
                        <Route path=path!("/r/:token") view=recs::RecLandingPage />
                        <Route path=path!("/admin") view=pages::AdminPage />
                        <Route path=path!("/watchlist") view=pages::WatchlistPage />
                        <Route path=path!("/recs") view=recs::RecsPage />
                        <Route path=path!("/signin") view=pages::SignInPage />
                        <Route path=path!("/auth/verify") view=pages::VerifyPage />
                        <Route path=path!("/privacy") view=pages::PrivacyPage />
                        <Route path=path!("/changelog") view=pages::ChangelogPage />
                        <Route path=path!("/about") view=pages::AboutPage />
                    </Routes>
                </main>

                <footer class="bg-sc-panel border-t border-sc-border py-8 text-center text-stone-600 text-sm">
                    <p>{move || d().footer_tagline}</p>
                    <p class="text-xs text-stone-700 mt-2 tracking-widest">
                        "// "
                        <a href="https://www.imdb.com/title/tt0076740/"
                           target="_blank" rel="noopener noreferrer"
                           class="hover:text-sc-accent-dim transition-colors">
                            "SORCERER, 1977 — WILLIAM FRIEDKIN"
                        </a>
                    </p>
                    <p class="text-xs text-stone-700 mt-3">
                        <A href="/about" attr:class="hover:text-stone-500 transition-colors">
                            {move || d().footer_how_it_works}
                        </A>
                        " · "
                        <A href="/privacy" attr:class="hover:text-stone-500 transition-colors">
                            {move || d().footer_privacy}
                        </A>
                        " · " {move || d().footer_no_cookies_ads} " · "
                        <a href="mailto:rk@rkzwei.dev" class="hover:text-stone-500 transition-colors">
                            "rk@rkzwei.dev"
                        </a>
                        " · "
                        <A href="/changelog" attr:class="hover:text-stone-500 transition-colors">
                            {concat!("v", env!("CARGO_PKG_VERSION"))}
                        </A>
                    </p>
                    <div class="flex justify-center mt-4">
                        <LocaleToggle />
                    </div>
                </footer>
            </div>
        </Router>
    }
}

/// `PT | EN` locale switch. Reads/writes the `Lang` context signal; the active
/// language is highlighted with the accent color.
#[component]
fn LocaleToggle() -> impl IntoView {
    let lang = i18n::use_lang();
    let cls = |active: bool| -> String {
        let base = "bg-transparent border-none p-0 cursor-pointer transition-colors text-xs sm:text-sm tracking-wide";
        if active {
            format!("{} text-sc-accent", base)
        } else {
            format!("{} text-stone-500 hover:text-stone-200", base)
        }
    };
    view! {
        <div class="flex items-center gap-1.5" aria-label="Language">
            <button
                class=move || cls(lang.get() == Lang::Pt)
                on:click=move |_| lang.set(Lang::Pt)
            >"PT"</button>
            <span class="text-stone-700 text-xs">"|"</span>
            <button
                class=move || cls(lang.get() == Lang::En)
                on:click=move |_| lang.set(Lang::En)
            >"EN"</button>
        </div>
    }
}
